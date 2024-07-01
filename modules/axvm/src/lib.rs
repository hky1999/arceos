//! [ArceOS](https://github.com/rcore-os/arceos) virtial machine monitor management module.
//!
//! This module provides primitives for VM management, including VM
//! creation, two-stage memory management, device emulation, etc.
//!
//! Currently `axvm` actes like a wrapper for [hypercraft](https://github.com/arceos-hypervisor/hypercraft.git).
//!
//! !!! This module is WORK-IN-PROCESS !!!

#![cfg_attr(not(test), no_std)]
#![feature(doc_cfg)]
#![feature(doc_auto_cfg)]
#![feature(exclusive_range_pattern)]

extern crate alloc;

#[macro_use]
extern crate log;
#[macro_use]
extern crate pci;

mod config;
// #[cfg(target_arch = "x86_64")]
mod device;
mod mm;

mod arch;

mod handlers;
mod hvc;
mod irq;
mod nmi;

mod vm;
pub use vm::*;

mod vcpu;
// mod vcpu_list;

pub use arch::{PerCpu, VCpu};

pub use axhal::mem::{phys_to_virt, virt_to_phys, PhysAddr};
pub use mm::GuestPageTable;

pub use hypercraft::GuestPageTableTrait;
pub use hypercraft::HyperCraftHal;
pub use hypercraft::HyperError as Error;
pub use hypercraft::HyperResult as Result;
#[cfg(feature = "type1_5")]
pub use hypercraft::LinuxContext;
pub use hypercraft::{
    GuestPhysAddr, GuestVirtAddr, HostPhysAddr, HostVirtAddr, HyperCallMsg, VmExitInfo,
};
pub use hypercraft::{PerCpuDevices, PerVmDevices, VmxExitReason};

use core::sync::atomic::{AtomicU32, AtomicUsize, Ordering};
use alloc::sync::Arc;

use crate::config::entry::VMCfgEntry;
use axhal::current_cpu_id;

// use super::type1_5::cell;
static INIT_GPM_OK: AtomicU32 = AtomicU32::new(0);
static INITED_CPUS: AtomicUsize = AtomicUsize::new(0);

pub fn boot_host_linux() {
    let hart_id = current_cpu_id();
    let mut vm_id = 0 as usize;
    let linux_context = axhal::hv::get_linux_context(hart_id);

    crate::arch::cpu_hv_hardware_enable(hart_id, linux_context)
        .expect("cpu_hv_hardware_enable failed");

    if hart_id == 0 {
        config::init_root_gpm().expect("init_root_gpm failed");
        let mvm_config = VMCfgEntry::new_host();
        vm_id = mvm_config.vm_id();
        let mut mvm = VM::new(Arc::new(mvm_config));
        vm::push_vm(vm_id, mvm);

        info!("Host VM new success");

        INIT_GPM_OK.store(1, Ordering::Release);
    } else {
        while INIT_GPM_OK.load(Ordering::Acquire) < 1 {
            core::hint::spin_loop();
        }
    }

    // let mut vm_list_lock = ;
    let mut mvm = vm::get_vm_by_id(vm_id).unwrap();

    info!("CPU{} add vcpu to vm...", hart_id);

    INITED_CPUS.fetch_add(1, Ordering::SeqCst);
    while INITED_CPUS.load(Ordering::Acquire) < axconfig::SMP {
        core::hint::spin_loop();
    }

    let vcpu = mvm.vcpu(hart_id).expect("VCPU not exist");

    debug!("CPU{} before run vcpu {}", hart_id, vcpu.id());
    info!("{:?}", vcpu.run());
}

