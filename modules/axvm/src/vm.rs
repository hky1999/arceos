use alloc::collections::BTreeMap;
use alloc::string::String;
use alloc::vec::Vec;
use core::ops::Deref;
use core::sync::atomic::{AtomicU32, AtomicUsize, Ordering};
use memory_addr::PAGE_SIZE_4K;
use spin::rwlock::RwLock;

use crate::vcpu::Vcpu;
use crate::vcpu_list::VmCpus;
use hypercraft::VCpu;
use hypercraft::VmxInterruptionType;

use super::arch::new_vcpu;
#[cfg(target_arch = "x86_64")]
use super::device::{self, GuestVMDevices, X64VcpuDevices, X64VmDevices};
use crate::GuestPageTable;
use alloc::sync::{Arc, Weak};
use axhal::{current_cpu_id, hv::HyperCraftHalImpl};

use crate::config::entry::{vm_cfg_entry, VMCfgEntry, VmType};
use crate::device::{BarAllocImpl, DeviceList};

use hashbrown::HashMap;
use lazy_static::lazy_static;
use spin::Mutex;

lazy_static! {
    pub static ref VCPU_TO_PCPU: Mutex<HashMap<(u32, u32), u32>> = Mutex::new(HashMap::new());
}

static VM_CNT: AtomicU32 = AtomicU32::new(0);

struct VMList {
    vm_list: BTreeMap<usize, Arc<VM>>,
}

impl VMList {
    const fn new() -> VMList {
        VMList {
            vm_list: BTreeMap::new(),
        }
    }
}

static GLOBAL_VM_LIST: Mutex<VMList> = Mutex::new(VMList::new());

// use super::type1_5::cell;
static INIT_GPM_OK: AtomicU32 = AtomicU32::new(0);
static INITED_CPUS: AtomicUsize = AtomicUsize::new(0);

pub fn vcpu2pcpu(vm_id: u32, vcpu_id: u32) -> Option<u32> {
    let lock = VCPU_TO_PCPU.lock();
    lock.get(&(vm_id, vcpu_id)).cloned()
}

pub fn map_vcpu2pcpu(vm_id: u32, vcpu_id: u32, pcup_id: u32) {
    let mut lock = VCPU_TO_PCPU.lock();
    lock.insert((vm_id, vcpu_id), pcup_id);
}

pub fn config_boot_linux() {
    let hart_id = current_cpu_id();
    let vm_id = 0 as usize;
    let linux_context = axhal::hv::get_linux_context();

    crate::arch::cpu_hv_hardware_enable(hart_id, linux_context)
        .expect("cpu_hv_hardware_enable failed");

    if hart_id == 0 {
        super::config::init_root_gpm().expect("init_root_gpm failed");
        let mvm_config = VMCfgEntry::default();
        let mut mvm = VM::new(vm_id, mvm_config);
        GLOBAL_VM_LIST.lock().vm_list.insert(vm_id, mvm);

        info!("CPU{} vm new success", hart_id);

        INIT_GPM_OK.store(1, Ordering::Release);
    } else {
        while INIT_GPM_OK.load(Ordering::Acquire) < 1 {
            core::hint::spin_loop();
        }
    }

    let mut vm_list_lock = GLOBAL_VM_LIST.lock();
    let mut mvm = vm_list_lock.vm_list.get(&vm_id).unwrap();

    info!("CPU{} add vcpu to vm...", hart_id);

    // map_vcpu2pcpu(vm_id, hart_id as u32, hart_id as u32);

    // let vcpu = mvm.get_vcpu(hart_id).expect("bind vcpu failed");

    // vcpu.bind_to_current_processor();
    INITED_CPUS.fetch_add(1, Ordering::SeqCst);
    while INITED_CPUS.load(Ordering::Acquire) < axconfig::SMP {
        core::hint::spin_loop();
    }

    let vcpu = mvm.vcpu(hart_id).expect("VCPU {} not exist");

    debug!("CPU{} before run vcpu", hart_id);
    info!("{:?}", mvm.run_type15_vcpu(hart_id, &linux_context));

    // disable hardware virtualization todo
}

pub fn boot_vm(vm_id: usize) {
    let hart_id = current_cpu_id();
    let vm_cfg_entry = match vm_cfg_entry(vm_id) {
        Some(entry) => entry,
        None => {
            warn!("VM {} not existed, boot vm failed", vm_id);
            return;
        }
    };

    info!(
        "boot_vm {} {:?} on core {}, guest entry {:#x}",
        vm_id,
        vm_cfg_entry.get_vm_type(),
        axhal::current_cpu_id(),
        vm_cfg_entry.get_vm_entry(),
    );

    // let gpm = vm_cfg_entry
    //     .generate_guest_phys_memory_set()
    //     .expect("Failed to generate GPM");

    // // let gpm = gpm.read().deref();
    // // let npt = gpm.nest_page_table();
    // // let npt_root = gpm.nest_page_table_root();

    // let npt_root = gpm.read().nest_page_table_root();
    // info!("{:#x?}", gpm);

    // let vm_id = VM_CNT.load(Ordering::SeqCst);
    // VM_CNT.fetch_add(1, Ordering::SeqCst);
    // let vcpu_id = 0;
    // debug!("create vcpu {} for vm {}", vcpu_id, vm_id);
    // // Main scheduling item, managed by `axtask`
    // let vcpu = VCpu::new(
    //     vcpu_id,
    //     crate::arch::cpu_vmcs_revision_id(),
    //     vm_cfg_entry.get_vm_entry(),
    //     npt_root,
    // )
    // .unwrap();
    // let mut vcpus =
    //     VmCpus::<HyperCraftHalImpl, X64VcpuDevices<HyperCraftHalImpl, BarAllocImpl>>::new();
    // vcpus.add_vcpu(vcpu).expect("add vcpu failed");

    // map_vcpu2pcpu(vm_id, vcpu_id as u32, hart_id as u32);

    // let mut vm = VM::<
    //     // HyperCraftHalImpl,
    //     X64VcpuDevices<HyperCraftHalImpl, BarAllocImpl>,
    //     // GuestVMDevices<HyperCraftHalImpl, BarAllocImpl>,
    //     // GuestPageTable,
    // >::new(vcpus, gpm, vm_id);
    // // The bind_vcpu method should be decoupled with vm struct.
    // vm.bind_vcpu(vcpu_id).expect("bind vcpu failed");

    // info!("Running guest...");
    // info!("{:?}", vm.run_vcpu(0));
}

use bit_set::BitSet;
use iced_x86::{Decoder, DecoderOptions, Formatter, Instruction, MasmFormatter, OpKind};
// use spin::RwLock;

use hypercraft::{
    GuestPageTableTrait, GuestPageWalkInfo, GuestPhysAddr, GuestVirtAddr, HostPhysAddr,
    HostVirtAddr, HyperCraftHal, HyperError, HyperResult, LinuxContext, PerCpuDevices,
    PerVmDevices, VmxExitReason,
};

use crate::mm::{AddressSpace, GuestPhysMemorySet};

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
    config: VMCfgEntry,
    vcpu_list: Box<[Vcpu]>,
    // emu_devs: Vec<Arc<dyn EmuDev<HyperCraftHalImpl>>>,
    devices: DeviceList<HyperCraftHalImpl, BarAllocImpl>,
}

impl VmInnerConst {
    fn new(vm_id: usize, config: VMCfgEntry, vm: Weak<VM>) -> Self {
        let phys_id_list = config.get_physical_id_list();
        debug!("VM[{}] vcpu phys_id_list {:?}", id, phys_id_list);

        let mut vcpu_list = Vec::with_capacity(config.cpu_num());
        for (vcpu_id, phys_id) in phys_id_list.into_iter().enumerate() {
            vcpu_list.push(Vcpu::new(vm.clone(), vcpu_id, phys_id, &config));
        }
        let mut this = Self {
            vm_id,
            config,
            vcpu_list: vcpu_list.into_boxed_slice(),
            // emu_devs: vec![],
            devices: DeviceList::new(Some(0), Some(vm_id as u32)),
        };
        this.init_devices(vm);
        this
    }

    fn init_devices(&mut self, vm: Weak<VM>) -> bool {
        true
    }
}

struct VmInnerMut {
    pub mem_set: GuestPhysMemorySet,
}

impl VmInnerMut {
    fn new(mem_set: GuestPhysMemorySet) -> Self {
        Self { mem_set }
    }
}

impl VM {
    /// Create a new [`VM`].
    pub fn new(vm_id: usize, config: VMCfgEntry) -> Arc<Self> {
        debug!("Constuct VM {vm_id}");
        let mem_set = config.generate_guest_phys_memory_set().unwrap();

        let this = Arc::new_cyclic(|weak| VM {
            inner_const: VmInnerConst::new(vm_id, config, weak.clone()),
            inner_mut: Mutex::new(VmInnerMut::new(mem_set)),
        });
        for vcpu in this.vcpu_list() {
            vcpu.init(this);
        }

        this
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
        self.inner_mut.lock().mem_set.nest_page_table_root()
    }

    // /// Bind the specified [`VCpu`] to current physical processor.
    // pub fn bind_vcpu(&mut self, vcpu_id: usize) -> HyperResult<&mut VCpu<HyperCraftHalImpl>> {
    //     match self.inner_const.vcpus.get_vcpu(vcpu_id) {
    //         Ok(vcpu) => {
    //             vcpu.bind_to_current_processor()?;
    //             Ok(vcpu)
    //         }
    //         e @ Err(_) => e,
    //     }
    // }

    // #[allow(unreachable_code)]
    // /// Run a specified [`VCpu`] on current logical vcpu.
    // pub fn run_vcpu(&mut self, vcpu_id: usize) -> HyperResult {
    //     let (vcpu, vcpu_device) = self.vcpus.get_vcpu_and_device(vcpu_id).unwrap();

    //     loop {
    //         if let Some(exit_info) = vcpu.run() {
    //             // we need to handle vm-exit this by ourselves

    //             if exit_info.exit_reason == VmxExitReason::VMCALL {
    //                 let regs = vcpu.regs();
    //                 trace!("{:#x?}", regs);
    //                 let id = regs.rax as u32;
    //                 let args = (regs.rdi as usize, regs.rsi as usize, regs.rdx as usize);

    //                 match vcpu_device.hypercall_handler(vcpu, id, args) {
    //                     Ok(result) => vcpu.regs_mut().rax = result as u64,
    //                     Err(e) => panic!("Hypercall failed: {e:?}, hypercall id: {id:#x}, args: {args:#x?}, vcpu: {vcpu:#x?}"),
    //                 }

    //                 vcpu.advance_rip(VM_EXIT_INSTR_LEN_VMCALL)?;
    //             } else if exit_info.exit_reason == VmxExitReason::EXCEPTION_NMI {
    //                 match vcpu_device.nmi_handler(vcpu) {
    //                     Ok(result) => vcpu.regs_mut().rax = result as u64,
    //                     Err(e) => panic!("nmi_handler failed: {e:?}"),
    //                 }
    //             } else {
    //                 let result = vcpu_device.vmexit_handler(vcpu, &exit_info).or_else(|| {
    //                     let guest_rip = exit_info.guest_rip;
    //                     let length = exit_info.exit_instruction_length;
    //                     let instr = Self::decode_instr(self.ept.clone(), vcpu, guest_rip, length)
    //                         .expect("decode instruction failed");
    //                     self.device.vmexit_handler(vcpu, &exit_info, Some(instr))
    //                 });

    //                 match result {
    //                     Some(result) => {
    //                         if result.is_err() {
    //                             panic!(
    //                                 "VM failed to handle a vm-exit: {:?}, error {:?}, vcpu: {:#x?}",
    //                                 exit_info.exit_reason,
    //                                 result.unwrap_err(),
    //                                 vcpu
    //                             );
    //                         }
    //                     }
    //                     None => {
    //                         if exit_info.exit_reason == VmxExitReason::IO_INSTRUCTION {
    //                             let io_info = vcpu.io_exit_info().unwrap();
    //                             panic!(
    //                                 "nobody wants to handle this vm-exit: {:#x?}, io-info: {:?}",
    //                                 exit_info, io_info
    //                             );
    //                         } else {
    //                             panic!(
    //                                 "nobody wants to handle this vm-exit: {:#x?}, vcpu: {:#x?}",
    //                                 exit_info, vcpu
    //                             );
    //                         }
    //                     }
    //                 }
    //             }
    //         }

    //         vcpu_device.check_events(vcpu)?;
    //     }

    //     Ok(())
    // }

    // #[cfg(feature = "type1_5")]
    // #[allow(unreachable_code)]
    // /// Run a specified [`VCpu`] on current logical vcpu.
    // pub fn run_type15_vcpu(&mut self, vcpu_id: usize, linux: &LinuxContext) -> HyperResult {
    //     let vcpu = self.get_vcpu(vcpu_id).unwrap();
    //     vcpu.bind_to_current_processor()?;
    //     loop {
    //         if let Some(exit_info) = vcpu.run_type15(linux) {
    //             match exit_info.exit_reason {
    //                 VmxExitReason::VMCALL => {
    //                     let regs = vcpu.regs();
    //                     let id = regs.rax as u32;
    //                     let args = (regs.rdi as usize, regs.rsi as usize, regs.rdx as usize);

    //                     trace!("{:#x?}", regs);
    //                     match self.hypercall_handler(vcpu, id, args) {
    //                         Ok(result) => vcpu.regs_mut().rax = result as u64,
    //                         Err(e) => panic!("Hypercall failed: {e:?}, hypercall id: {id:#x}, args: {args:#x?}, vcpu: {vcpu:#x?}"),
    //                     }

    //                     vcpu.advance_rip(VM_EXIT_INSTR_LEN_VMCALL)?;
    //                 }
    //                 VmxExitReason::EXCEPTION_NMI => match self.nmi_handler(vcpu) {
    //                     Ok(result) => vcpu.regs_mut().rax = result as u64,
    //                     Err(e) => panic!("nmi_handler failed: {e:?}"),
    //                 },

    //                 VmxExitReason::EPT_VIOLATION => {
    //                     let guest_rip = exit_info.guest_rip;
    //                     let length = exit_info.exit_instruction_length;
    //                     let instr = Self::decode_instr(
    //                         Arc::new(self.inner_mut.get_mut().mem_set.nest_page_table()),
    //                         vcpu,
    //                         guest_rip,
    //                         length,
    //                     )
    //                     .expect("decode instruction failed");
    //                     self.inner_const
    //                         .devices
    //                         .handle_mmio_instruction(vcpu, &exit_info, Some(instr))
    //                         .unwrap()?;
    //                 }
    //                 VmxExitReason::IO_INSTRUCTION => self
    //                     .inner_const
    //                     .devices
    //                     .handle_io_instruction(vcpu, &exit_info)
    //                     .unwrap()?,
    //                 VmxExitReason::MSR_READ => self.inner_const.devices.handle_msr_read(vcpu)?,
    //                 VmxExitReason::MSR_WRITE => self.inner_const.devices.handle_msr_write(vcpu)?,
    //                 _ => {
    //                     panic!(
    //                         "nobody wants to handle this vm-exit: {:#x?}, vcpu: {:#x?}",
    //                         exit_info, vcpu
    //                     );
    //                 }
    //             }
    //         }
    //     }
    // }

    // /// Unbind the specified [`VCpu`] bond by [`VM::<H>::bind_vcpu`].
    // pub fn unbind_vcpu(&mut self, vcpu_id: usize) -> HyperResult {
    //     if self.vcpu_bond.contains(vcpu_id) {
    //         match self.vcpus.get_vcpu_and_device(vcpu_id) {
    //             Ok((vcpu, _)) => {
    //                 self.vcpu_bond.remove(vcpu_id);
    //                 vcpu.unbind_from_current_processor()?;
    //                 Ok(())
    //             }
    //             Err(e) => Err(e),
    //         }
    //     } else {
    //         Err(HyperError::InvalidParam)
    //     }
    // }

    // /// Get per-vm devices.
    // pub fn devices(&mut self) -> &mut GuestVMDevices<HyperCraftHalImpl, BarAllocImpl> {
    //     &mut self.device
    // }

    /// decode guest instruction
    pub fn decode_instr(
        &self,
        vcpu: &VCpu<HyperCraftHalImpl>,
        guest_rip: usize,
        length: u32,
    ) -> HyperResult<Instruction> {
        let asm = self
            .inner_mut
            .lock()
            .mem_set
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

pub fn hypercall_handler(
    vcpu: &mut VCpu<HyperCraftHalImpl>,
    id: u32,
    args: (usize, usize, usize),
) -> HyperResult<u32> {
    // debug!("hypercall #{id:#x?}, args: {args:#x?}");
    crate::hvc::handle_hvc(vcpu, id as usize, args)
}

pub fn nmi_handler(vcpu: &mut VCpu<HyperCraftHalImpl>) -> HyperResult<u32> {
    use crate::nmi::{NmiMessage, CORE_NMI_LIST};
    let current_cpu_id = current_cpu_id();
    let current_core_id = axhal::cpu_id_to_core_id(current_cpu_id);
    let msg = CORE_NMI_LIST[current_core_id].lock().pop();
    match msg {
        Some(NmiMessage::BootVm(vm_id)) => {
            crate::vm::boot_vm(vm_id);
            Ok(0)
        }
        None => {
            warn!(
                "CPU [{}] (Processor [{}])NMI VM-Exit",
                current_cpu_id, current_core_id
            );
            let int_info = vcpu.interrupt_exit_info()?;
            warn!(
                "interrupt_exit_info:{:#x}\n{:#x?}\n{:#x?}",
                vcpu.raw_interrupt_exit_info()?,
                int_info,
                vcpu
            );

            if int_info.int_type == VmxInterruptionType::NMI {
                unsafe { core::arch::asm!("int 2") }
            } else {
                // Reinject the event straight away.
                debug!(
                    "reinject to VM on CPU {} Processor {}",
                    current_cpu_id, current_core_id
                );
                vcpu.queue_event(int_info.vector, int_info.err_code);
            }
            // // System Control Port A (0x92)
            // // BIT	Description
            // // 4*	Watchdog timer status

            // let value = unsafe { x86::io::inb(0x92) };
            // warn!("System Control Port A value {:#x}", value);
            // // System Control Port B (0x61)
            // // Bit	Description
            // // 6*	Channel check
            // // 7*	Parity check
            // // The Channel Check bit indicates a failure on the bus,
            // // probably by a peripheral device such as a modem, sound card, NIC, etc,
            // // while the Parity check bit indicates a memory read or write failure.
            // let value = unsafe { x86::io::inb(0x61) };
            // warn!("System Control Port B value {:#x}", value);
            Ok(0)
        }
    }
}

pub fn external_interrupt_handler(vcpu: &mut VCpu<HyperCraftHalImpl>) -> HyperResult {
    let int_info = vcpu.interrupt_exit_info()?;
    debug!("VM-exit: external interrupt: {:#x?}", int_info);

    if int_info.vector != 0xf0 {
        panic!("VM-exit: external interrupt: {:#x?}", int_info);
    }

    assert!(int_info.valid);

    crate::irq::dispatch_host_irq(int_info.vector as usize)
}
