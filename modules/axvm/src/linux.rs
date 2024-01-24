/// Temporar module to boot Linux as a guest VM.
///
/// To be removed...
// use hypercraft::GuestPageTableTrait;
use hypercraft::{PerCpu, VCpu, VmCpus, VM};

#[cfg(target_arch = "x86_64")]
use super::device::{self, X64VcpuDevices, X64VmDevices};

use super::hal::HyperCraftHalImpl;

use crate::arch::pcpu;

pub fn config_linux(hart_id: usize) {
    info!("into main {}", hart_id);

    crate::arch::pcpu::cpu_hv_enable_hardware(hart_id);

    let gpm = super::config::setup_gpm(hart_id).unwrap();
    let npt = gpm.nest_page_table_root();
    info!("{:#x?}", gpm);

    let vcpu = VCpu::new(0, crate::arch::pcpu::cpu_vmcs_revision_id(), 0x7c00, npt).unwrap();

    let mut vcpus = VmCpus::<HyperCraftHalImpl, X64VcpuDevices<HyperCraftHalImpl>>::new();
    vcpus.add_vcpu(vcpu).expect("add vcpu failed");

    let mut vm = VM::<
        HyperCraftHalImpl,
        X64VcpuDevices<HyperCraftHalImpl>,
        X64VmDevices<HyperCraftHalImpl>,
    >::new(vcpus);
    vm.bind_vcpu(0).expect("bind vcpu failed");

    if hart_id == 0 {
        let (_, dev) = vm.get_vcpu_and_device(0).unwrap();
        *(dev.console.lock().backend()) = device::device_emu::MultiplexConsoleBackend::Primary;

        for v in 0..256 {
            crate::irq::set_host_irq_enabled(v, true);
        }
    }

    info!("Running guest...");
    info!("{:?}", vm.run_vcpu(0));

    crate::arch::pcpu::cpu_hv_hardware_disable();

    panic!("done");
}
