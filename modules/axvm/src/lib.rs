//! [ArceOS](https://github.com/rcore-os/arceos) virtial machine monitor management module.
//!
//! This module provides primitives for VM management, including VM
//! creation, two-stage memory management, device emulation, etc.
//!
//! This module is WORK-IN-PROCESS.

#![cfg_attr(not(test), no_std)]
#![feature(doc_cfg)]
#![feature(doc_auto_cfg)]

extern crate alloc;

#[macro_use]
extern crate log;

mod config;
#[cfg(target_arch = "x86_64")]
mod device;
mod mm;

mod hal;
mod page_table;

mod irq;

/// To be removed.
mod linux;
pub use linux::linux;

pub use axhal::mem::{phys_to_virt, virt_to_phys, PhysAddr};
pub use hal::HyperCraftHalImpl;
pub use page_table::GuestPageTable;

pub use hypercraft::GuestPageTableTrait;

pub use hypercraft::HyperCraftHal;
pub use hypercraft::HyperError as Error;
pub use hypercraft::HyperResult as Result;
#[cfg(not(target_arch = "aarch64"))]
pub use hypercraft::{
    GuestPhysAddr, GuestVirtAddr, HostPhysAddr, HostVirtAddr, HyperCallMsg, VmExitInfo,
};
pub use hypercraft::{PerCpu, VCpu, VmCpus, VM};
#[cfg(target_arch = "x86_64")]
pub use hypercraft::{PerCpuDevices, PerVmDevices, VmxExitReason};

