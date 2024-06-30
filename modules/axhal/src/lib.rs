//! [ArceOS] hardware abstraction layer, provides unified APIs for
//! platform-specific operations.
//!
//! It does the bootstrapping and initialization process for the specified
//! platform, and provides useful operations on the hardware.
//!
//! Currently supported platforms (specify by cargo features):
//!
//! - `platform-pc-x86`: Standard PC with x86_64 ISA.
//! - `platform-qemu-virt-riscv`: QEMU virt machine with RISC-V ISA.
//! - `platform-qemu-virt-aarch64`: QEMU virt machine with AArch64 ISA.
//! - `dummy`: If none of the above platform is selected, the dummy platform
//!    will be used. In this platform, most of the operations are no-op or
//!    `unimplemented!()`. This platform is mainly used for [cargo test].
//!
//! # Cargo Features
//!
//! - `smp`: Enable SMP (symmetric multiprocessing) support.
//! - `fp_simd`: Enable floating-point and SIMD support.
//! - `paging`: Enable page table manipulation.
//! - `irq`: Enable interrupt handling support.
//! - `platform-pc-x86`: Specify for use on the corresponding platform.
//! - `platform-qemu-virt-riscv`: Specify for use on the corresponding platform.
//! - `platform-qemu-virt-aarch64`: Specify for use on the corresponding platform.
//!
//! [ArceOS]: https://github.com/rcore-os/arceos
//! [cargo test]: https://doc.rust-lang.org/cargo/guide/tests.html

#![no_std]
#![feature(asm_const)]
#![feature(naked_functions)]
#![feature(doc_auto_cfg)]
#![feature(inline_const)]

#[allow(unused_imports)]
#[macro_use]
extern crate log;

pub const KERNEL_PROCESS_ID: u64 = 1;

mod platform;

pub mod arch;
pub mod cpu;
pub mod mem;
pub mod time;
pub mod trap;

#[cfg(feature = "irq")]
pub mod irq;

#[cfg(feature = "paging")]
pub mod paging;

#[cfg(feature = "hv")]
pub mod hv;

/// Console input and output.
pub mod console {
    pub use super::platform::console::*;

    /// Write a slice of bytes to the console.
    pub fn write_bytes(bytes: &[u8]) {
        for c in bytes {
            putchar(*c);
        }
    }
}

/// Miscellaneous operation, e.g. terminate the system.
pub mod misc {
    pub use super::platform::misc::*;
}

/// Multi-core operations.
#[cfg(feature = "smp")]
pub mod mp {
    pub use super::platform::mp::*;
}

pub use self::platform::platform_init;

#[cfg(feature = "smp")]
pub use self::platform::platform_init_secondary;

#[cfg(feature = "type1_5")]
pub mod header {
    pub use super::platform::header::*;
}

#[cfg(feature = "type1_5")]
pub mod config {
    pub use super::platform::config::*;
}
#[cfg(feature = "type1_5")]
pub mod consts {
    pub use super::platform::consts::*;
}

pub use platform::current_cpu_id;
#[cfg(target_arch="x86_64")]
pub use platform::core_id_to_cpu_id;
#[cfg(target_arch="x86_64")]
pub use platform::cpu_id_to_core_id;
