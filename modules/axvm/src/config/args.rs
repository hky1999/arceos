use alloc::string::String;
use alloc::vec::Vec;
use hypercraft::{GuestPhysAddr, HostPhysAddr};
use serde::{Deserialize, Serialize};

// The struct used for parameter passing between the kernel module and ArceOS hypervisor.
// This struct should have the same memory layout as the `AxVMCreateArg` structure in ArceOS.
// See jailhouse-arceos/driver/axvm.h
#[derive(Debug)]
#[repr(C, packed)]
pub struct AxVMCreateArg {
    /// VM ID, set by ArceOS hypervisor.
    pub vm_id: usize,
    // /// Reserved.
    // vm_type: usize,
    // /// VM cpu mask.
    // cpu_mask: usize,
    // /// VM entry point.
    // vm_entry_point: GuestPhysAddr,

    // /// BIOS image loaded target guest physical address.
    // bios_load_gpa: GuestPhysAddr,
    // /// Kernel image loaded target guest physical address.
    // kernel_load_gpa: GuestPhysAddr,
    // /// randisk image loaded target guest physical address.
    // ramdisk_load_gpa: GuestPhysAddr,
    pub raw_cfg_base: GuestPhysAddr,
    _raw_cfg_size: usize,

    /// Physical load address of BIOS, set by ArceOS hypervisor.
    pub bios_load_hpa: HostPhysAddr,
    /// Physical load address of kernel image, set by ArceOS hypervisor.
    pub kernel_load_hpa: HostPhysAddr,
    /// Physical load address of ramdisk image, set by ArceOS hypervisor.
    pub ramdisk_load_hpa: HostPhysAddr,
}

#[derive(Debug, Default, Serialize, Deserialize)]
pub struct VmMemCfg {
    pub gpa: usize,
    pub size: usize,
    pub flags: usize,
}

#[derive(Debug, Default, Serialize, Deserialize)]
pub struct VmCreateCliArg {
    // Basic Information
    pub id: usize,
    pub name: String,
    pub vm_type: usize,
    // Resources.
    pub cpu_set: usize,

    pub entry_point: usize,

    pub bios_path: String,
    pub bios_load_addr: usize,

    pub kernel_path: String,
    pub kernel_load_addr: usize,

    pub ramdisk_path: Option<String>,
    pub ramdisk_load_addr: Option<usize>,

    pub disk_path: Option<String>,

    /// Memory Information
    pub memory_regions: Vec<VmMemCfg>,
}
