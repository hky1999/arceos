use alloc::boxed::Box;
use arrayvec::ArrayVec;
use spin::Once;

use super::arch::VCpu;
use crate::Result;
use axhal::hv::HyperCraftHalImpl;

pub use hypercraft::{HyperCraftHal, PerCpuDevices, PerVmDevices, VmCpus, VM};

// Todo: refactor this, move to arch?
#[cfg(target_arch = "x86_64")]
use super::device::{self, X64VcpuDevices, X64VmDevices};

/// The maximum number of CPUs we can support.
pub const MAX_CPUS: usize = 8;

pub const VM_CPUS_MAX: usize = MAX_CPUS;

pub struct VMInner<H: HyperCraftHal, PD: PerCpuDevices<H>, VD: PerVmDevices<H>> {
    vm: VM<H, PD, VD>,
    vcpus: VmCpus<H, PD>,
    inner: [Once<VCpu<H>>; VM_CPUS_MAX],
    device: [Once<PD>; VM_CPUS_MAX],
}

impl<H: HyperCraftHal, PD: PerCpuDevices<H>, VD: PerVmDevices<H>> VMInner<H, PD, VD> {
    pub fn new(vcpus: VmCpus<H, PD>) -> Self {
        Self {
            vm: VM::<H, PD, VD>::new(vcpus),
            vcpus,
        }
    }

    pub fn bind_cpu(&mut self) {
        self.vm.bind_vcpu(0).expect("bind vcpu failed");
    }

    pub fn run(&mut self) {
        crate::arch::cpu_hv_hardware_enable(hart_id);
        // if hart_id == 0 {
        let (_, dev) = self.vm.get_vcpu_and_device(0).unwrap();
        *(dev.console.lock().backend()) = device::device_emu::MultiplexConsoleBackend::Primary;

        for v in 0..256 {
            crate::irq::set_host_irq_enabled(v, true);
        }
        // }
        info!("Running guest...");
        self.vm.run_vcpu(0);

        crate::arch::cpu_hv_hardware_disable();

        panic!("done");
    }
}
