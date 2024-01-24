macro_rules! cfg_block {
    ($( #[$meta:meta] {$($item:item)*} )*) => {
        $($(
            #[$meta]
            $item
        )*)*
    }
}

// about guests
pub const BIOS_PADDR: HostPhysAddr = 0x400_0000;
pub const BIOS_SIZE: usize = 0x2000;
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

mod gpm_def;
pub use gpm_def::setup_gpm;