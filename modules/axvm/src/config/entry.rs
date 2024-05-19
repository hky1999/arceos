use alloc::collections::BTreeMap;
use alloc::string::String;
use alloc::sync::Arc;
use alloc::vec::Vec;

use spin::Mutex;
use spin::RwLock;

use axalloc::GlobalPage;
use axhal::mem::virt_to_phys;
use hypercraft::{GuestPhysAddr, HostPhysAddr, HostVirtAddr};
use memory_addr::PAGE_SIZE_4K;
use page_table_entry::MappingFlags;

use crate::mm::{GuestMemoryRegion, GuestPhysMemorySet};
use crate::{Error, Result};

// VM_ID = 0 reserved for host Linux.
const CONFIG_VM_ID_START: usize = 1;
const CONFIG_VM_NUM_MAX: usize = 8;

#[inline]
const fn align_up_4k(pos: usize) -> usize {
    (pos + PAGE_SIZE_4K - 1) & !(PAGE_SIZE_4K - 1)
}

struct VmConfigTable {
    entries: BTreeMap<usize, Arc<VMCfgEntry>>,
}

impl VmConfigTable {
    const fn new() -> VmConfigTable {
        VmConfigTable {
            entries: BTreeMap::new(),
        }
    }

    fn generate_vm_id(&mut self) -> Result<usize> {
        for i in CONFIG_VM_ID_START..CONFIG_VM_NUM_MAX {
            if !self.entries.contains_key(&i) {
                return Ok(i);
            }
        }
        Err(Error::OutOfRange)
    }

    fn remove_vm_by_id(&mut self, vm_id: usize) {
        if vm_id >= CONFIG_VM_NUM_MAX || !self.entries.contains_key(&vm_id) {
            error!("illegal vm id {}", vm_id);
        } else {
            self.entries.remove(&vm_id);
        }
    }
}

#[derive(Default, Clone, Copy, PartialEq, Eq, Debug)]
pub enum VmType {
    #[default]
    VmTUnknown = 0,
    VmTNimbOS = 1,
    VmTLinux = 2,
}

impl From<usize> for VmType {
    fn from(value: usize) -> Self {
        match value {
            0 => Self::VmTUnknown,
            1 => Self::VmTNimbOS,
            2 => Self::VmTLinux,
            _ => panic!("Unknown VmType value: {}", value),
        }
    }
}

#[derive(Debug, Default)]
struct VMImgCfg {
    kernel_load_gpa: GuestPhysAddr,
    vm_entry_point: GuestPhysAddr,
    bios_load_gpa: GuestPhysAddr,
    ramdisk_load_gpa: GuestPhysAddr,

    kernel_load_hpa: HostPhysAddr,
    bios_load_hpa: HostPhysAddr,
    ramdisk_load_hpa: HostPhysAddr,
}

impl VMImgCfg {
    pub fn new(
        kernel_load_gpa: GuestPhysAddr,
        vm_entry_point: GuestPhysAddr,
        bios_load_gpa: GuestPhysAddr,
        ramdisk_load_gpa: GuestPhysAddr,
    ) -> VMImgCfg {
        VMImgCfg {
            kernel_load_gpa,
            vm_entry_point,
            bios_load_gpa,
            ramdisk_load_gpa,
            kernel_load_hpa: 0xdead_beef,
            bios_load_hpa: 0xdead_beef,
            ramdisk_load_hpa: 0xdead_beef,
        }
    }
}

#[derive(Debug)]
pub struct VMCfgEntry {
    vm_id: usize,
    name: String,
    vm_type: VmType,

    cmdline: String,
    /// The cpu_set here refers to the `core_id` from Linux's perspective. \
    /// Therefore, when looking for the corresponding `cpu_id`, 
    /// we need to perform a conversion using `core_id_to_cpu_id`.
    cpu_set: usize,

    img_cfg: VMImgCfg,

    memory_regions: Vec<GuestMemoryRegion>,
    physical_pages: BTreeMap<usize, GlobalPage>,
    memory_set: Option<Arc<RwLock<GuestPhysMemorySet>>>,
}

impl VMCfgEntry {
    pub fn new(
        name: String,
        vm_type: VmType,
        cmdline: String,
        cpu_set: usize,
        kernel_load_gpa: GuestPhysAddr,
        vm_entry_point: GuestPhysAddr,
        bios_load_gpa: GuestPhysAddr,
        ramdisk_load_gpa: GuestPhysAddr,
    ) -> Self {
        VMCfgEntry {
            vm_id: 0xdeaf_beef,
            name,
            vm_type,
            cmdline,
            cpu_set,
            img_cfg: VMImgCfg::new(
                kernel_load_gpa,
                vm_entry_point,
                bios_load_gpa,
                ramdisk_load_gpa,
            ),
            memory_regions: Vec::new(),
            physical_pages: BTreeMap::new(),
            memory_set: None,
        }
    }

    pub fn get_cpu_set(&self) -> usize {
        self.cpu_set
    }

    pub fn get_vm_type(&self) -> VmType {
        self.vm_type
    }

    pub fn get_vm_entry(&self) -> GuestPhysAddr {
        self.img_cfg.vm_entry_point
    }

    pub fn add_physical_pages(&mut self, index: usize, pages: GlobalPage) {
        self.physical_pages.insert(index, pages);
    }

    pub fn memory_region_editor<F>(&mut self, f: F)
    where
        F: FnOnce(&mut Vec<GuestMemoryRegion>),
    {
        f(&mut self.memory_regions)
    }

    /// Set up VM GuestMemoryRegion.
    /// Alloc physical memory region for guest ram memory.
    pub fn set_up_memory_region(&mut self) -> Result {
        for (index, region) in self.memory_regions.iter_mut().enumerate() {
            // We do not need to alloc physical memory region for device regions.
            if region.flags.contains(MappingFlags::DEVICE) {
                continue;
            }
            let ram_size = align_up_4k(region.size);
            let physical_pages =
                GlobalPage::alloc_contiguous(ram_size / PAGE_SIZE_4K, PAGE_SIZE_4K).map_err(
                    |e| {
                        warn!(
                            "failed to allocate {} Bytes memory for guest, err {:?}",
                            ram_size, e
                        );
                        Error::NoMemory
                    },
                )?;
            let ram_base_hpa = physical_pages.start_paddr(virt_to_phys).as_usize();
            region.hpa = ram_base_hpa;

            debug!(
                "Alloc {:#x} Bytes of GlobalPage for ram region\n{}",
                physical_pages.size(),
                region
            );

            self.physical_pages.insert(index, physical_pages);
        }

        Ok(())
    }

    pub fn generate_guest_phys_memory_set(&mut self) -> Result<Arc<RwLock<GuestPhysMemorySet>>> {
        info!("Create VM [{}] nested page table", self.vm_id);

        // create nested page table and add mapping
        let mut gpm = GuestPhysMemorySet::new()?;
        for r in &self.memory_regions {
            gpm.map_region(r.clone().into())?;
        }
        let result = Arc::new(RwLock::new(gpm));
        self.memory_set = Some(result.clone());
        Ok(result)
    }

    fn gpa_to_hpa_inside_ram_memory_region(&self, addr: GuestPhysAddr) -> Option<HostPhysAddr> {
        for (index, region) in self.memory_regions.iter().enumerate() {
            if region.flags.contains(MappingFlags::DEVICE) {
                continue;
            }
            if ((region.gpa..region.gpa + region.size).contains(&addr)) {
                debug!("Target GuestPhysAddr {:#x} belongs to \n\t{}", addr, region);
                return self
                    .physical_pages
                    .get(&index)
                    .map(|pages| pages.start_paddr(virt_to_phys).as_usize() + addr - region.gpa);
            }
        }

        return None;
    }

    /// According to the VM configuration,
    /// find the `HostPhysAddr` to which each Guest VM image needs to be loaded.
    /// Return Value:
    ///   bios_load_hpa : HostPhysAddr
    ///   kernel_load_hpa : HostPhysAddr
    ///   ramdisk_load_hpa : HostPhysAddr
    pub fn get_img_load_info(&mut self) -> (HostPhysAddr, HostPhysAddr, HostPhysAddr) {
        if let Some(bios_load_hpa) =
            self.gpa_to_hpa_inside_ram_memory_region(self.img_cfg.bios_load_gpa)
        {
            self.img_cfg.bios_load_hpa = bios_load_hpa;
        } else {
            warn!(
                "Guest VM bios load gpa {:#x} not in ram memory range",
                self.img_cfg.bios_load_gpa
            );
            self.img_cfg.bios_load_hpa = 0;
        }

        if let Some(kernel_load_hpa) =
            self.gpa_to_hpa_inside_ram_memory_region(self.img_cfg.kernel_load_gpa)
        {
            self.img_cfg.kernel_load_hpa = kernel_load_hpa;
        } else {
            warn!(
                "Guest VM kernel load gpa {:#x} not in ram memory range",
                self.img_cfg.kernel_load_gpa,
            );
            self.img_cfg.kernel_load_hpa = 0;
        }

        if let Some(ramdisk_load_hpa) =
            self.gpa_to_hpa_inside_ram_memory_region(self.img_cfg.ramdisk_load_gpa)
        {
            self.img_cfg.ramdisk_load_hpa = ramdisk_load_hpa;
        } else {
            warn!(
                "Guest VM ramdisk load gpa {:#x} not in ram memory range",
                self.img_cfg.ramdisk_load_gpa,
            );
            self.img_cfg.ramdisk_load_hpa = 0;
        }

        debug!(
            "bios_load_hpa {:#x} kernel_load_hpa {:#x} ramdisk_load_hpa {:#x}",
            self.img_cfg.bios_load_hpa, self.img_cfg.kernel_load_hpa, self.img_cfg.ramdisk_load_hpa,
        );

        (
            self.img_cfg.bios_load_hpa,
            self.img_cfg.kernel_load_hpa,
            self.img_cfg.ramdisk_load_hpa,
        )
    }
}

static GLOBAL_VM_CFG_TABLE: Mutex<VmConfigTable> = Mutex::new(VmConfigTable::new());

pub fn vm_cfg_entry(vm_id: usize) -> Option<Arc<VMCfgEntry>> {
    let vm_configs = GLOBAL_VM_CFG_TABLE.lock();
    return vm_configs.entries.get(&vm_id).cloned();
}

/* Add VM config entry to DEF_VM_CONFIG_TABLE
 *
 * @param[in] vm_cfg_entry: new added VM config entry.
 * @param[out] vm_id: the VM id of newly added VM.
 */
pub fn vm_cfg_add_vm_entry(mut vm_cfg_entry: VMCfgEntry) -> Result<usize> {
    let mut vm_configs = GLOBAL_VM_CFG_TABLE.lock();
    let vm_id = vm_configs.generate_vm_id()?;
    vm_cfg_entry.vm_id = vm_id;
    vm_configs.entries.insert(vm_id, Arc::new(vm_cfg_entry));
    Ok(vm_id)
}
