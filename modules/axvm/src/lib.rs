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

mod mm;
mod config;
#[cfg(target_arch = "x86_64")]
mod device;

mod hal;
mod page_table;

/// To be removed.
mod linux;
pub use linux::linux;

pub use axhal::mem::{phys_to_virt, virt_to_phys, PhysAddr};
pub use page_table::GuestPageTable;
pub use hal::HyperCraftHalImpl;

pub use hypercraft::GuestPageTableTrait;

pub use hypercraft::HyperError as Error;
pub use hypercraft::HyperResult as Result;
pub use hypercraft::HyperCraftHal;
pub use hypercraft::{PerCpu, VCpu, VmCpus, VM};
#[cfg(not(target_arch = "aarch64"))]
pub use hypercraft::{HyperCallMsg, VmExitInfo, GuestPhysAddr, GuestVirtAddr, HostPhysAddr, HostVirtAddr};
#[cfg(target_arch = "x86_64")]
pub use hypercraft::{PerCpuDevices, PerVmDevices, VmxExitReason};

#[cfg(target_arch = "x86_64")]
pub fn dispatch_host_irq(vector: usize) -> Result {
    #[cfg(feature = "irq")] 
    {
        axhal::irq::dispatch_irq(vector);
        Ok(())
    }
    #[cfg(not(feature = "irq"))] 
    {
        panic!("cannot handle EXTERNAL_INTERRUPT vmexit because \"irq\" is not enabled")
    }
}

#[cfg(target_arch = "x86_64")]
pub fn set_host_irq_enabled(vector: usize, enabled: bool) -> Result {
    #[cfg(feature = "irq")] 
    {
        axhal::irq::set_enable(vector, enabled);
        Ok(())
    }
    #[cfg(not(feature = "irq"))] 
    {
        panic!("cannot call set_host_irq_enabled because \"irq\" is not enabled")
    }
}

