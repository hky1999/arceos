mod entry;

mod percpu;

mod config;
mod consts;
mod header;
mod uart16550;

pub mod mem;
pub mod misc;
pub mod time;

#[cfg(feature = "irq")]
pub mod irq {
    pub use super::apic::*;
}

pub mod console {
    pub use super::uart16550::*;
}

use config::HvSystemConfig;
// use error::HvResult;
// use header::HvHeader;
use percpu::PerCpu;

extern "C" {
    fn rust_main(cpu_id: usize, dtb: usize) -> !;
    #[cfg(feature = "smp")]
    fn rust_main_secondary(cpu_id: usize) -> !;
}

extern "sysv64" fn rust_entry(cpu_data: &mut PerCpu, linux_sp: usize) -> i32 {
    let is_primary = cpu_data.id == 0;

    axlog::ax_println!(
        "{} CPU {} entered, linux sp at {:#x}.",
        if is_primary { "Primary" } else { "Secondary" },
        cpu_data.id,
        linux_sp
    );

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
    unsafe {
        rust_main(cpu_data.id as usize, 0);
    }
    // let code = 0;
    // axlog::ax_println!(
    //     "CPU {} return back to driver with code {}.",
    //     cpu_data.id,
    //     code
    // );
    // code
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
