use crate::{GuestPhysAddr, HostPhysAddr};

macro_rules! cfg_block {
    ($( #[$meta:meta] {$($item:item)*} )*) => {
        $($(
            #[$meta]
            $item
        )*)*
    }
}

// about guests

pub const NIMBOS_VM_ENTRY: GuestPhysAddr = 0x8000;
pub const NIMBOS_BIOS_LOAD_GPA: GuestPhysAddr = 0x8000;
pub const NIMBOS_KERNEL_LOAD_GPA: GuestPhysAddr = 0x20_0000;

// Hardcoded in `apps/hv/guest/vlbl/entry.S`.
pub const LINUX_VM_ENTRY: GuestPhysAddr = 0x7c00;
pub const LINUX_BIOS_LOAD_GPA: GuestPhysAddr = 0x7c00;
pub const LINUX_KERNEL_LOAD_GPA: GuestPhysAddr = 0x70200000;
pub const LINUX_RAMDISK_LOAD_GPA: GuestPhysAddr = 0x72000000;

cfg_block! {
    #[cfg(feature = "guest_nimbos")]
    {
        pub const BIOS_ENTRY: GuestPhysAddr = 0x8000;
        pub const GUEST_ENTRY: GuestPhysAddr = 0x20_0000;

        pub const GUEST_IMAGE_PADDR: HostPhysAddr = 0x400_1000;
        pub const GUEST_IMAGE_SIZE: usize = 0x10_0000; // 1M
    }
    #[cfg(feature = "guest_linux")]
    {
        pub const BIOS_ENTRY: GuestPhysAddr = 0x7c00;
    }
}
pub const GUEST_PHYS_MEMORY_BASE: GuestPhysAddr = 0;
pub const GUEST_PHYS_MEMORY_SIZE: usize = 0x100_0000; // 16M

#[cfg(not(feature = "type1_5"))]
#[path = "gpm_def.rs"]
mod gpm_def;
#[cfg(feature = "type1_5")]
#[path = "gpm_def_type15.rs"]
mod gpm_def;

#[cfg(feature = "type1_5")]
pub use gpm_def::{init_root_gpm, root_gpm};

pub mod linux_cfg_def;
pub mod nimbos_cfg_def;

pub(crate) mod args;

mod entry;
pub use entry::*;
