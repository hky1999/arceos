/// Physical CPU config for virtualization support.
use lazy_init::LazyInit;

use hypercraft::{HostPhysAddr, HostVirtAddr, HyperCraftHal};
use x86::msr::P5_MC_ADDR;

pub use hypercraft::PerCpu;

use axhal::hv::HyperCraftHalImpl;
use crate::Result;

#[percpu::def_percpu]
static HV_PER_CPU: LazyInit<PerCpu<HyperCraftHalImpl>> = LazyInit::new();

pub fn cpu_hv_hardware_enable(hart_id: usize) -> Result {
    info!("Core [{hart_id}] init hardware support for virtualization...");

    let per_cpu = unsafe { HV_PER_CPU.current_ref_mut_raw() };
    if !per_cpu.is_init() {
        per_cpu.init_by(PerCpu::<HyperCraftHalImpl>::new(hart_id));
    }

    per_cpu.hardware_enable()
}

pub fn cpu_hv_hardware_disable() -> Result {
    let per_cpu = unsafe { HV_PER_CPU.current_ref_mut_raw() };
    assert!(
        per_cpu.is_init(),
        "Per CPU structure is not intialized before!"
    );

    per_cpu.hardware_disable()
}

pub fn cpu_vmcs_revision_id() -> u32 {
    let per_cpu = unsafe { HV_PER_CPU.current_ref_mut_raw() };
    assert!(
        per_cpu.is_init(),
        "Per CPU structure is not intialized before!"
    );
    per_cpu.vmcs_revision_id()
}
