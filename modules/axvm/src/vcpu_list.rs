use alloc::boxed::Box;
use arrayvec::ArrayVec;
use spin::Once;

// use crate::arch::{VCpu, VM};
// use crate::hal::PerCpuDevices;
use crate::{Error, GuestPageTableTrait, HyperCraftHal, Result, VM};

use axhal::{current_cpu_id, hv::HyperCraftHalImpl};
use hypercraft::{PerCpuDevices, VCpu};

/// The maximum number of CPUs we can support.
pub const MAX_CPUS: usize = 8;

pub const VM_CPUS_MAX: usize = MAX_CPUS;

/// The set of vCPUs in a VM.
#[derive(Default)]
pub struct VmCpus {
    inner: [Once<VCpu<HyperCraftHalImpl>>; VM_CPUS_MAX],
    // device: [Once<PD>; VM_CPUS_MAX],
}

impl VmCpus {
    /// Creates a new vCPU tracking structure.
    pub fn new() -> Self {
        Self {
            inner: [Once::INIT; VM_CPUS_MAX],
            // device: [Once::INIT; VM_CPUS_MAX],
        }
    }

    /// Adds the given vCPU to the set of vCPUs.
    pub fn add_vcpu(&mut self, vcpu: VCpu<HyperCraftHalImpl>) -> Result {
        let vcpu_id = vcpu.vcpu_id();
        let once_entry = self.inner.get(vcpu_id).ok_or(Error::BadState)?;

        let _real_vcpu = once_entry.call_once(|| vcpu);
        // let device_once_entry = self.device.get(vcpu_id).ok_or(Error::BadState)?;

        // device_once_entry.call_once(|| PD::new(real_vcpu).unwrap());

        Ok(())
    }

    /// Returns a reference to the vCPU with `vcpu_id` if it exists.
    pub fn get_vcpu(&mut self, vcpu_id: usize) -> Result<&mut VCpu<HyperCraftHalImpl>> {
        let vcpu = self
            .inner
            .get_mut(vcpu_id)
            .and_then(|once| once.get_mut())
            .ok_or(Error::NotFound)?;
        Ok(vcpu)
    }
}

// Safety: Each VCpu is wrapped with a Mutex to provide safe concurrent access to VCpu.
// unsafe impl<PD: PerCpuDevices<HyperCraftHalImpl>> Sync for VmCpus<PD> {}
// unsafe impl<PD: PerCpuDevices<HyperCraftHalImpl>> Send for VmCpus<PD> {}
