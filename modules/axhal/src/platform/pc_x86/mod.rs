mod apic;
#[cfg(not(feature = "type1_5"))]
#[path = "boot.rs"]
mod boot;
#[cfg(feature = "type1_5")]
#[path = "boot_type15.rs"]
mod boot;
mod dtables;
mod uart16550;

#[cfg(feature = "monolithic")]
pub(crate) use dtables::{kernel_stack_top, set_kernel_stack_top};

pub mod mem;
pub mod misc;
pub mod time;

#[cfg(feature = "smp")]
pub mod mp;

#[cfg(feature = "irq")]
pub mod irq {
    pub use super::apic::*;
}

pub mod console {
    pub use super::uart16550::*;
}

extern "C" {
    fn rust_main(cpu_id: usize, dtb: usize) -> !;
    #[cfg(feature = "smp")]
    fn rust_main_secondary(cpu_id: usize) -> !;
}

const MAX_CORE_ID: u32 = 254;

/**
 * In ArceOS, the `cpu_id` refers to the APIC ID.
 * However, Linux has its own perspective on `core_id`.
 * Here, we perform a simple conversion using a global array.
 */
static mut CORE_ID_TO_CPU_ID: [usize; MAX_CORE_ID as usize + 1] =
    [usize::MAX; MAX_CORE_ID as usize + 1];

pub fn set_core_id_to_cpu_id(core_id: usize, cpu_id: usize) {
    unsafe { CORE_ID_TO_CPU_ID[core_id as usize] = cpu_id };
}

pub fn core_id_to_cpu_id(core_id: usize) -> Option<usize> {
    let cpu_id = unsafe { CORE_ID_TO_CPU_ID[core_id as usize] };
    if cpu_id == usize::MAX {
        warn!("Core [{}] not registered!!!", core_id);
        None
    } else {
        Some(cpu_id)
    }
}

pub fn cpu_id_to_core_id(cpu_id: usize) -> usize {
    let mut core_id: usize = 0;
    while core_id < MAX_CORE_ID as usize {
        if unsafe { CORE_ID_TO_CPU_ID[core_id] } == cpu_id {
            return core_id;
        }
        core_id += 1;
    }
    panic!("CPU [{}] not registered!!!", cpu_id);
}

pub fn current_cpu_id() -> usize {
    match raw_cpuid::CpuId::new().get_feature_info() {
        Some(finfo) => finfo.initial_local_apic_id() as usize,
        None => 0,
    }
}

#[cfg(not(feature = "type1_5"))]
unsafe extern "C" fn rust_entry(magic: usize, _mbi: usize) {
    // TODO: handle multiboot info
    if magic == self::boot::MULTIBOOT_BOOTLOADER_MAGIC {
        crate::mem::clear_bss();
        crate::cpu::init_primary(current_cpu_id());
        self::uart16550::init();
        self::dtables::init_primary();
        self::time::init_early();
        rust_main(current_cpu_id(), 0);
    }
}

#[cfg(not(feature = "type1_5"))]
#[allow(unused_variables)]
unsafe extern "C" fn rust_entry_secondary(magic: usize) {
    #[cfg(feature = "smp")]
    if magic == self::boot::MULTIBOOT_BOOTLOADER_MAGIC {
        crate::cpu::init_secondary(current_cpu_id());
        self::dtables::init_secondary();
        rust_main_secondary(current_cpu_id());
    }
}

/// Initializes the platform devices for the primary CPU.
pub fn platform_init() {
    self::apic::init_primary();
    #[cfg(not(feature = "type1_5"))]
    {
        self::time::init_primary();
    }
}

#[cfg(feature = "type1_5")]
pub mod config;
#[cfg(feature = "type1_5")]
pub mod consts;
#[cfg(feature = "type1_5")]
pub mod header;

#[cfg(feature = "type1_5")]
pub mod context;

/// Initializes the platform devices for secondary CPUs.
#[cfg(feature = "smp")]
pub fn platform_init_secondary() {
    #[cfg(not(feature = "type1_5"))]
    {
        self::apic::init_secondary();
        self::time::init_secondary();
    }
}

#[cfg(feature = "type1_5")]
use core::sync::atomic::{AtomicU32, Ordering};
#[cfg(feature = "type1_5")]
static INIT_EARLY_OK: AtomicU32 = AtomicU32::new(0);
#[cfg(feature = "type1_5")]
static BOOTED_CPUS: AtomicU32 = AtomicU32::new(0);

#[cfg(feature = "type1_5")]
// hypervisor start
extern "sysv64" fn rust_entry_hv(core_id: u32, linux_sp: usize) -> i32 {
    BOOTED_CPUS.fetch_add(1, Ordering::SeqCst);

    while BOOTED_CPUS.load(Ordering::Acquire) < crate::header::HvHeader::get().online_cpus {
        core::hint::spin_loop();
    }

    axlog::ax_println!("Core {} enter rust entry hv!!!", core_id);

    if core_id == 0 {
        primary_init_early(core_id, linux_sp);
    } else {
        while INIT_EARLY_OK.load(Ordering::Acquire) < 1 {
            core::hint::spin_loop();
        }
        secondary_init_early(core_id, linux_sp);
    }
    let ret = unsafe { rust_main(core_id as usize, 0) };
    ret
}

#[cfg(feature = "type1_5")]
fn primary_init_early(core_id: u32, linux_sp: usize) {
    // crate::mem::clear_bss();
    crate::cpu::init_primary(core_id as usize);

    // This should be called after `percpu` is init.
    // This should be called before operations related to dtables
    // to get a clean unmodified Linux context.
    context::save_linux_context(core_id as _, linux_sp);

    self::uart16550::init();
    self::dtables::init_primary();
    self::time::init_early();
    self::mem::init_mmio_num();
    axlog::ax_println!("primary_init_early OK!!!");
    INIT_EARLY_OK.store(1, Ordering::Release);
}

#[cfg(feature = "type1_5")]
fn secondary_init_early(core_id: u32, linux_sp: usize) {
    crate::cpu::init_secondary(core_id as _);
    // This should be called after `percpu` is init.
    // This should be called before operations related to dtables
    // to get a clean unmodified Linux context.
    context::save_linux_context(core_id as _, linux_sp);
    self::dtables::init_secondary();
}
