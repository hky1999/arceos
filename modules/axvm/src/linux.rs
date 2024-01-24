/// Temporar module to boot Linux as a guest VM.
///
/// To be removed...

// use hypercraft::GuestPageTableTrait;
use hypercraft::{PerCpu, VCpu, VmCpus, VM};


#[cfg(target_arch = "x86_64")]
use super::device::{self, X64VcpuDevices, X64VmDevices};

use super::hal::HyperCraftHalImpl;

pub fn linux(hart_id: usize) {
    
    info!("into main {}", hart_id);

    let mut p = PerCpu::<HyperCraftHalImpl>::new(hart_id);
    p.hardware_enable().unwrap();

    let gpm = super::config::setup_gpm(hart_id).unwrap();
    let npt = gpm.nest_page_table_root();
    info!("{:#x?}", gpm);

    let mut vcpus = VmCpus::<HyperCraftHalImpl, X64VcpuDevices<HyperCraftHalImpl>>::new();
    vcpus.add_vcpu(VCpu::new(0, p.vmcs_revision_id(), 0x7c00, npt).unwrap()).expect("add vcpu failed");

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

    p.hardware_disable().unwrap();

    panic!("done");
}
