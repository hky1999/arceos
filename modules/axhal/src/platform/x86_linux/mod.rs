mod apic;
mod dtables;
mod entry;
mod uart16550;

pub mod mem;
pub mod misc;
pub mod time;

// mods for vmm usage.
mod percpu;

mod config;
mod consts;
mod header;

#[cfg(feature = "smp")]
pub mod mp;

#[cfg(feature = "irq")]
pub mod irq {
    pub use super::apic::*;
}

pub mod console {
    pub use super::uart16550::*;
}

use core::sync::atomic::{AtomicI32, AtomicU32, Ordering};

use config::HvSystemConfig;
// use error::HvResult;
use header::HvHeader;
use percpu::PerCpu;

static PRIMARY_INIT_OK: AtomicU32 = AtomicU32::new(0);
static ARCEOS_MAIN_INIT_OK: AtomicU32 = AtomicU32::new(0);
static ERROR_NUM: AtomicI32 = AtomicI32::new(0);

fn has_err() -> bool {
    ERROR_NUM.load(Ordering::Acquire) != 0
}

fn wait_for(condition: impl Fn() -> bool) {
    while !has_err() && condition() {
        core::hint::spin_loop();
    }
    if has_err() {
        axlog::ax_println!("[Error] Other cpu init failed!")
    }
}

fn wait_for_counter(counter: &AtomicU32, max_value: u32) {
    wait_for(|| counter.load(Ordering::Acquire) < max_value)
}

extern "C" {
    fn rust_main(cpu_id: usize, dtb: usize) -> !;
    #[cfg(feature = "smp")]
    fn rust_main_secondary(cpu_id: usize) -> !;
}

fn current_cpu_id() -> usize {
    match raw_cpuid::CpuId::new().get_feature_info() {
        Some(finfo) => finfo.initial_local_apic_id() as usize,
        None => 0,
    }
}

fn primary_init_early() {
    let cpu_id = current_cpu_id();

    crate::mem::clear_bss();
    crate::cpu::init_primary(cpu_id);
    self::uart16550::init();
    // self::dtables::init_primary();
    // self::time::init_early();

    let system_config = HvSystemConfig::get();

    axlog::ax_println!(
        "\n\
        Initializing hypervisor...\n\
        config_signature = {:?}\n\
        config_revision = {}\n\
        ",
        core::str::from_utf8(&system_config.signature),
        system_config.revision,
    );

    axlog::ax_println!("ARCEOS CPU {} primary_init_early() ok", cpu_id);

    PRIMARY_INIT_OK.store(1, Ordering::Release);
}

fn secondary_init_early() {
    let cpu_id = current_cpu_id();
    crate::cpu::init_secondary(cpu_id);

    axlog::ax_println!("ARCEOS CPU {} secondary_init_early() ok", cpu_id);
}

extern "sysv64" fn vm_cpu_entry(cpu_data: &mut PerCpu, linux_sp: usize) -> i32 {
    let is_primary = cpu_data.id == 0;
    let online_cpus = HvHeader::get().online_cpus;

    wait_for(|| PerCpu::entered_cpus() < online_cpus);

    if is_primary {
        axlog::ax_println!(
            "{} CPU {} entered.\n header {:#?}",
            "Linux",
            cpu_data.id,
            HvHeader::get()
        );
        primary_init_early();
        unsafe {
            rust_main(cpu_data.id as usize, 0);
        }
    } else {
        wait_for_counter(&ARCEOS_MAIN_INIT_OK, 1);
        axlog::ax_println!(
            "{} CPU {} entered.",
            if is_primary { "Primary" } else { "Secondary" },
            cpu_data.id
        );
        secondary_init_early();
        unsafe {
            rust_main_secondary(cpu_data.id as usize);
        }
    }

    let code = 0;
    axlog::info!(
        "CPU {} return back to driver with code {}.",
        cpu_data.id,
        code
    );
    code
}

/// Initializes the platform devices for the primary CPU.
pub fn platform_init() {
    // self::apic::init_primary();
    // self::time::init_primary();
}

/// Initializes the platform devices for secondary CPUs.
#[cfg(feature = "smp")]
pub fn platform_init_secondary() {
    // self::apic::init_secondary();
    // self::time::init_secondary();
}
