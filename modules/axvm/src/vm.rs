use alloc::boxed::Box;
use alloc::collections::BTreeMap;
use alloc::string::String;
use alloc::vec::Vec;
use core::ops::{Add, Deref};

use bit_set::BitSet;
use hashbrown::HashMap;
use iced_x86::{Decoder, DecoderOptions, Formatter, Instruction, MasmFormatter, OpKind};
use lazy_static::lazy_static;
use spin::rwlock::RwLock;
use spin::Mutex;

use crate::vcpu::Vcpu;
use hypercraft::VCpu;
use hypercraft::{
    GuestPageTableTrait, GuestPageWalkInfo, GuestPhysAddr, GuestVirtAddr, HostPhysAddr,
    HostVirtAddr, HyperCraftHal, LinuxContext, PerCpuDevices, PerVmDevices, VmxExitReason,
};
use memory_addr::{align_up_4k, PAGE_SIZE_4K};
use page_table_entry::MappingFlags;

use crate::config::{vm_cfg_entry, VMCfgEntry, VmType};
use crate::device::{self, BarAllocImpl, DeviceList};
use crate::mm::{AddressSpace, GuestPhysMemorySet};
use crate::GuestPageTable;
use crate::{Error, Result};
use alloc::sync::{Arc, Weak};
use axalloc::GlobalPage;
use axhal::{current_cpu_id, hv::HyperCraftHalImpl, mem::virt_to_phys};

lazy_static! {
    pub static ref VCPU_TO_PCPU: Mutex<HashMap<(u32, u32), u32>> = Mutex::new(HashMap::new());
}

struct VMList {
    vm_list: BTreeMap<usize, Arc<VM>>,
}

impl VMList {
    const fn new() -> VMList {
        VMList {
            vm_list: BTreeMap::new(),
        }
    }

    fn push_vm(&mut self, vm_id: usize, vm: Arc<VM>) {
        if (self.vm_list.contains_key(&vm_id)) {
            warn!(
                "VM[{}] already exists, push VM failed, just return ...",
                vm_id
            );
            return;
        }
        self.vm_list.insert(vm_id, vm);
    }

    #[allow(unused)]
    fn remove_vm(&mut self, vm_id: usize) -> Option<Arc<VM>> {
        self.vm_list.remove(&vm_id)
    }

    fn get_vm_by_id(&self, vm_id: usize) -> Option<Arc<VM>> {
        self.vm_list.get(&vm_id).cloned()
    }
}

static GLOBAL_VM_LIST: Mutex<VMList> = Mutex::new(VMList::new());

pub fn push_vm(vm_id: usize, vm: Arc<VM>) {
    GLOBAL_VM_LIST.lock().push_vm(vm_id, vm)
}

#[allow(unused)]
pub fn remove_vm(vm_id: usize) -> Option<Arc<VM>> {
    GLOBAL_VM_LIST.lock().remove_vm(vm_id)
}

#[allow(unused)]
pub fn get_vm_by_id(vm_id: usize) -> Option<Arc<VM>> {
    GLOBAL_VM_LIST.lock().get_vm_by_id(vm_id)
}

pub fn vcpu2pcpu(vm_id: u32, vcpu_id: u32) -> Option<u32> {
    let lock = VCPU_TO_PCPU.lock();
    lock.get(&(vm_id, vcpu_id)).cloned()
}

pub fn map_vcpu2pcpu(vm_id: u32, vcpu_id: u32, pcup_id: u32) {
    let mut lock = VCPU_TO_PCPU.lock();
    lock.insert((vm_id, vcpu_id), pcup_id);
}

const VM_EXIT_INSTR_LEN_VMCALL: u8 = 3;

/// VM define.
pub struct VM {
    // vcpus: VmCpus<PD>,
    // vcpu_bond: BitSet,
    // device: GuestVMDevices<HyperCraftHalImpl, BarAllocImpl>,
    // vm_id: u32,
    // /// EPT
    // pub ept: Arc<GuestPageTable>,
    // pub sys_mem: Arc<AddressSpace>,
    inner_const: VmInnerConst,
    inner_mut: Mutex<VmInnerMut>,
}

struct VmInnerConst {
    vm_id: usize,
    config: Arc<VMCfgEntry>,
    vcpu_list: Box<[Vcpu]>,
    // emu_devs: Vec<Arc<dyn EmuDev<HyperCraftHalImpl>>>,
    devices: DeviceList<HyperCraftHalImpl, BarAllocImpl>,
}

impl VmInnerConst {
    fn new(config: Arc<VMCfgEntry>, address_space: Arc<AddressSpace>, vm: Weak<VM>) -> Self {
        let vm_id = config.vm_id();
        let phys_id_list = config.get_physical_id_list();
        debug!("VM[{}] vcpu phys_id_list {:?}", vm_id, phys_id_list);

        let mut vcpu_list = Vec::with_capacity(config.cpu_num());
        for (vcpu_id, phys_id) in phys_id_list.into_iter().enumerate() {
            vcpu_list.push(Vcpu::new(vm.clone(), vcpu_id, phys_id, &config));
        }
        let mut this = Self {
            vm_id,
            config,
            vcpu_list: vcpu_list.into_boxed_slice(),
            // emu_devs: vec![],
            devices: DeviceList::new(vm_id),
        };

        this.init_devices(address_space);
        this
    }

    fn init_devices(&mut self, address_space: Arc<AddressSpace>) {
        self.devices
            .init(self.config.clone(), address_space)
            .unwrap()
    }
}

struct VmInnerMut {
    address_space: Arc<AddressSpace>,
    physical_pages: BTreeMap<usize, GlobalPage>,
}

impl VmInnerMut {
    fn new(config: Arc<VMCfgEntry>) -> Self {
        let gpm = match config.get_vm_type() {
            VmType::VMTHostVM => crate::config::root_gpm().clone(),
            _ => GuestPhysMemorySet::new().unwrap(),
        };

        Self {
            address_space: Arc::new(AddressSpace::new(gpm)),
            physical_pages: BTreeMap::new(),
        }
    }

    fn setup_memory_regions(&mut self, config: Arc<VMCfgEntry>) -> Result {
        for cfg_region in config.get_memory_regions() {
            // We do not need to alloc physical memory region for device regions.
            if cfg_region.flags.contains(MappingFlags::DEVICE) {
                continue;
            }
            let ram_size = align_up_4k(cfg_region.size);
            let pages = GlobalPage::alloc_contiguous(ram_size / PAGE_SIZE_4K, PAGE_SIZE_4K)
                .map_err(|e| {
                    warn!(
                        "failed to allocate {} Bytes memory for guest, err {:?}",
                        ram_size, e
                    );
                    Error::NoMemory
                })?;
            let ram_base_hpa = pages.start_paddr(virt_to_phys).as_usize();
            // region.hpa = ram_base_hpa;

            debug!(
                "Alloc {:#x} Bytes of GlobalPage for ram region\n{}",
                pages.size(),
                cfg_region
            );

            self.physical_pages.insert(ram_base_hpa, pages);

            let mut region = cfg_region.clone();
            region.hpa = ram_base_hpa;

            self.address_space.map_region(region);
        }
        Ok(())
    }
}

impl VM {
    /// Create a new [`VM`].
    pub fn new(config: Arc<VMCfgEntry>) -> Result<Arc<Self>> {
        debug!(
            "Constuct VM[{}] {} cpu_set {:#x}",
            config.vm_id(),
            config.vm_name(),
            config.get_cpu_set()
        );
        // let mem_set = config.get_guest_phys_memory_set().unwrap();

        let this = Arc::new_cyclic(|weak| {
            let mut inner_mut = VmInnerMut::new(config.clone());
            inner_mut.setup_memory_regions(config.clone());

            let inner_const =
                VmInnerConst::new(config, inner_mut.address_space.clone(), weak.clone());

            VM {
                inner_const,
                inner_mut: Mutex::new(inner_mut),
            }
        });

        this.setup_vcpus();

        Ok(this)
    }

    #[inline]
    pub fn id(&self) -> usize {
        self.inner_const.vm_id
    }

    #[inline]
    pub fn vcpu(&self, vcpu_id: usize) -> Option<&Vcpu> {
        self.vcpu_list().get(vcpu_id)
    }

    #[inline]
    pub fn vcpu_list(&self) -> &[Vcpu] {
        &self.inner_const.vcpu_list
    }

    #[inline]
    pub fn config(&self) -> &VMCfgEntry {
        &self.inner_const.config
    }

    #[inline]
    pub fn vm_type(&self) -> VmType {
        self.config().get_vm_type()
    }

    pub fn devices(&self) -> &DeviceList<HyperCraftHalImpl, BarAllocImpl> {
        &self.inner_const.devices
    }

    pub fn nest_page_table_root(&self) -> HostPhysAddr {
        self.inner_mut.lock().address_space.nest_page_table_root()
    }

    /// Setup Vcpu Context for each vcpu.
    fn setup_vcpus(&self) -> Result {
        for vcpu in self.vcpu_list() {
            vcpu.init(self)?;
        }
        Ok(())
    }

    /// According to the VM configuration,
    /// find the `HostPhysAddr` to which each Guest VM image needs to be loaded.
    /// Return Value:
    ///   bios_load_hpa : HostPhysAddr
    ///   kernel_load_hpa : HostPhysAddr
    ///   ramdisk_load_hpa : HostPhysAddr
    pub fn get_img_load_info(&self) -> Result<(HostPhysAddr, HostPhysAddr, HostPhysAddr)> {
        let (bioa_load_gpa, kernel_load_gpa, ramdisk_load_gpa) =
            self.config().get_img_load_info_gpa();

        let bios_load_hpa = self
            .inner_mut
            .lock()
            .address_space
            .translate_to_hpa(bioa_load_gpa)?;
        let kernel_load_hpa = self
            .inner_mut
            .lock()
            .address_space
            .translate_to_hpa(kernel_load_gpa)?;
        let ramdisk_load_hpa = if ramdisk_load_gpa != 0 {
            self.inner_mut
                .lock()
                .address_space
                .translate_to_hpa(ramdisk_load_gpa)?
        } else {
            0
        };

        debug!(
            "bios_load_hpa {:#x} kernel_load_hpa {:#x} ramdisk_load_hpa {:#x}",
            bios_load_hpa, kernel_load_hpa, ramdisk_load_hpa,
        );

        Ok((bios_load_hpa, kernel_load_hpa, ramdisk_load_hpa))
    }

    /// decode guest instruction
    pub fn decode_instr(
        &self,
        vcpu: &VCpu<HyperCraftHalImpl>,
        guest_rip: usize,
        length: u32,
    ) -> Result<Instruction> {
        let asm = self
            .inner_mut
            .lock()
            .address_space
            .get_gva_content_bytes(guest_rip, length, vcpu)?;
        let asm_slice = asm.as_slice();
        // Only one isntruction
        let mut decoder = Decoder::with_ip(64, asm_slice, guest_rip as u64, DecoderOptions::NONE);
        let instr = decoder.decode();
        // print instruction
        let mut output = String::new();
        let mut formatter = MasmFormatter::new();
        formatter.format(&instr, &mut output);
        // debug!("Instruction: {}", output);
        Ok(instr)
    }
}

pub fn setup_vm(vm_id: usize) -> Result<Arc<VM>> {
    let vm_cfg = match vm_cfg_entry(vm_id) {
        Some(entry) => entry,
        None => {
            warn!("VM {} not existed, boot vm failed", vm_id);
            return Err(Error::InvalidParam);
        }
    };

    info!(
        "setup_vm {} {:?} on core {}, guest entry {:#x}",
        vm_id,
        vm_cfg.get_vm_type(),
        axhal::current_cpu_id(),
        vm_cfg.get_vm_entry(),
    );

    let vm = VM::new(vm_cfg).map(|vm| {
        push_vm(vm_id, vm.clone());
        vm
    });

    vm
}

pub fn boot_vm(vm_id: usize) {
    if let Some(vm) = get_vm_by_id(vm_id) {
        let vcpu_id = 0;
        let vcpu = vm.vcpu(vcpu_id).expect("VCPU not exist");

        debug!("CPU{} before run vcpu {}", current_cpu_id(), vcpu.id());

        info!("{:?}", vcpu.run());
    } else {
        error!("VM [{}] not existed", vm_id);
    }
}
