use alloc::collections::BTreeMap;
use alloc::string::String;
use alloc::sync::Arc;
use alloc::vec;
use alloc::vec::Vec;

use spin::Mutex;
use spin::Once;
use spin::RwLock;

use axalloc::GlobalPage;
use axhal::mem::virt_to_phys;
use hypercraft::{GuestPhysAddr, HostPhysAddr, HostVirtAddr};
use memory_addr::{align_up_4k, PAGE_SIZE_4K};
use page_table_entry::MappingFlags;

use crate::mm::{GuestMemoryRegion, GuestPhysMemorySet};
use crate::{Error, Result};

// VM_ID = 0 reserved for host Linux.
const CONFIG_VM_ID_START: usize = 1;
const CONFIG_VM_NUM_MAX: usize = 8;

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
    VMTHostVM = 0,
    VmTNimbOS = 1,
    VmTLinux = 2,
}

impl From<usize> for VmType {
    fn from(value: usize) -> Self {
        match value {
            0 => Self::VMTHostVM,
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

#[derive(Debug, Default)]
pub struct VMCfgEntry {
    vm_id: usize,
    name: String,
    vm_type: VmType,

    cmdline: String,
    /// The cpu_set here refers to the `core_id` from Linux's perspective.
    /// Therefore, when looking for the corresponding `cpu_id`,
    /// we need to perform a conversion using `core_id_to_cpu_id`.
    cpu_set: usize,
    img_cfg: VMImgCfg,

    memory_regions: Vec<GuestMemoryRegion>,
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
        }
    }

    pub fn new_host() -> Self {
        VMCfgEntry {
            vm_id: 0,
            name: String::from("Host OS"),
            vm_type: VmType::VMTHostVM,
            cpu_set: {
                let mut set = 1;
                for _bit in 0..axconfig::SMP - 1 {
                    set <<= 1;
                    set |= 1;
                }
                debug!("Construct Host Config, SMP {} set {}", axconfig::SMP, set);
                set
            },
            ..Default::default()
        }
    }

    pub fn vm_id(&self) -> usize {
        self.vm_id
    }

    pub fn vm_name(&self) -> &str {
        self.name.as_str()
    }

    pub fn get_cpu_set(&self) -> usize {
        self.cpu_set
    }

    pub fn cpu_num(&self) -> usize {
        let mut cpu_num = 0;
        let mut cpu_set = self.cpu_set;
        while cpu_set != 0 {
            if cpu_set & 1 != 0 {
                cpu_num += 1;
            }
            cpu_set >>= 1;
        }
        cpu_num
    }

    pub fn get_physical_id_list(&self) -> Vec<usize> {
        let mut cpu_set = self.cpu_set;
        let mut phys_id_list = vec![];
        let mut phys_id = 0;
        while cpu_set != 0 {
            if cpu_set & 1 != 0 {
                phys_id_list.push(phys_id);
            }
            phys_id += 1;
            cpu_set >>= 1;
        }
        phys_id_list
    }

    pub fn get_vm_type(&self) -> VmType {
        self.vm_type
    }

    pub fn get_vm_entry(&self) -> GuestPhysAddr {
        self.img_cfg.vm_entry_point
    }

    /// Get image loading info in **guest physical address (GPA)** from VM configuration,
    /// Return Value:
    ///   bios_load_gpa : HostPhysAddr
    ///   kernel_load_gpa : HostPhysAddr
    ///   ramdisk_load_gpa : HostPhysAddr
    pub fn get_img_load_info_gpa(&self) -> (GuestPhysAddr, GuestPhysAddr, GuestPhysAddr) {
        (
            self.img_cfg.bios_load_gpa,
            self.img_cfg.kernel_load_gpa,
            self.img_cfg.ramdisk_load_gpa,
        )
    }

    pub fn append_memory_region(&mut self, region: GuestMemoryRegion) {
        self.memory_regions.push(region)
    }

    pub fn get_memory_regions(&self) -> Vec<GuestMemoryRegion> {
        self.memory_regions.clone()
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
 */
pub fn vm_cfg_add_vm_entry(mut vm_cfg: VMCfgEntry) -> Result<usize> {
    let mut vm_configs = GLOBAL_VM_CFG_TABLE.lock();
    let vm_id = vm_configs.generate_vm_id()?;
    vm_cfg.vm_id = vm_id;
    vm_configs.entries.insert(vm_id, Arc::new(vm_cfg));
    Ok(vm_id)
}
