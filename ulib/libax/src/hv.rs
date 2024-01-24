//! Hypervisor related functions

pub use axhal::mem::{phys_to_virt, virt_to_phys, PhysAddr};
pub use axruntime::GuestPageTable;
pub use axruntime::HyperCraftHalImpl;

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
