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

use axlog::ax_println as println;

use config::HvSystemConfig;
// use error::HvResult;
use header::HvHeader;
use percpu::PerCpu;

static VMM_PRIMARY_INIT_OK: AtomicU32 = AtomicU32::new(0);
static VMM_MAIN_INIT_OK: AtomicU32 = AtomicU32::new(0);
static INIT_LATE_OK: AtomicU32 = AtomicU32::new(0);
static ERROR_NUM: AtomicI32 = AtomicI32::new(0);

fn has_err() -> bool {
    ERROR_NUM.load(Ordering::Acquire) != 0
}

fn wait_for(condition: impl Fn() -> bool) {
    while !has_err() && condition() {
        core::hint::spin_loop();
    }
    if has_err() {
        println!("[Error] Other cpu init failed!")
    }
}

fn wait_for_counter(counter: &AtomicU32, max_value: u32) {
    wait_for(|| counter.load(Ordering::Acquire) < max_value)
}

extern "C" {
    fn rust_vmm_main(cpu_id: usize);
    #[cfg(feature = "smp")]
    fn rust_vmm_main_secondary(cpu_id: usize);
}

fn current_cpu_id() -> usize {
    match raw_cpuid::CpuId::new().get_feature_info() {
        Some(finfo) => finfo.initial_local_apic_id() as usize,
        None => 0,
    }
}

fn vmm_primary_init_early(cpu_id: usize) {
    crate::mem::clear_bss();
    // crate::cpu::init_secondary(current_cpu_id());
    // self::uart16550::init();
    // self::dtables::init_primary();
    // self::time::init_early();

    println!("HvHeader\n{:#?}", HvHeader::get());

    let system_config = HvSystemConfig::get();

    println!(
        "\n\
        Initializing hypervisor on Core [{}]...\n\
        config_signature = {:?}\n\
        config_revision = {}\n\
        ",
        cpu_id,
        core::str::from_utf8(&system_config.signature),
        system_config.revision,
    );

    VMM_PRIMARY_INIT_OK.store(1, Ordering::Release);

    unsafe {
        rust_vmm_main(cpu_id);
    }
}

fn vmm_secondary_init_early(cpu_id: usize) {
    println!("ARCEOS CPU {} secondary_init_early()", cpu_id);
    // crate::cpu::init_secondary(cpu_id);
    // self::dtables::init_secondary();
    unsafe {
        rust_vmm_main_secondary(cpu_id);
    }
}

extern "sysv64" fn vmm_cpu_entry(cpu_data: &mut PerCpu, _linux_sp: usize, linux_tp: usize) -> i32 {
    // Currently we set core 0 as Linux.
    let is_primary = cpu_data.id == 0;

    let vm_cpus = HvHeader::get().reserved_cpus();

    wait_for(|| PerCpu::entered_cpus() < vm_cpus);

    println!(
        "{} CPU {} entered.",
        if is_primary { "Primary" } else { "Secondary" },
        cpu_data.id
    );

    // First, we init primary core for VMM.
    if is_primary {
        vmm_primary_init_early(cpu_data.id as usize);
    } else {
        wait_for_counter(&VMM_PRIMARY_INIT_OK, 1);

        wait_for_counter(&VMM_MAIN_INIT_OK, 1);

        vmm_secondary_init_early(cpu_data.id as usize);
    }

    unsafe {
        x86::msr::wrmsr(x86::msr::IA32_GS_BASE, linux_tp as u64);
    }

    let code = 0;
    // println!(
    //     "{} CPU {} return back to driver with code {}.",
    //     if is_primary { "Primary" } else { "Secondary" },
    //     cpu_data.id,
    //     code
    // );
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
