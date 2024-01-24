/// Temporar module to boot Linux as a guest VM.
///
/// To be removed...

use axlog::info;

use axhal::mem::{phys_to_virt, virt_to_phys, PhysAddr};
use axruntime::GuestPageTable;
use axruntime::HyperCraftHalImpl;
use hypercraft::GuestPageTableTrait;

use hypercraft::HyperError as Error;
use hypercraft::HyperResult as Result;
use hypercraft::HyperCraftHal;
use hypercraft::{PerCpu, VCpu, VmCpus, VM};
#[cfg(not(target_arch = "aarch64"))]
use hypercraft::{HyperCallMsg, VmExitInfo, GuestPhysAddr, GuestVirtAddr, HostPhysAddr, HostVirtAddr};
#[cfg(target_arch = "x86_64")]
use hypercraft::{PerCpuDevices, PerVmDevices, VmxExitReason};

#[cfg(target_arch = "x86_64")]
use super::device::{X64VcpuDevices, X64VmDevices};

pub fn linux(hart_id: usize) {
    
    info!("into main {}", hart_id);

    let mut p = PerCpu::<HyperCraftHalImpl>::new(hart_id);
    p.hardware_enable().unwrap();

    let gpm = super::mm::mapper::setup_gpm(hart_id).unwrap();
    let npt = gpm.nest_page_table_root();
    info!("{:#x?}", gpm);

    let mut vcpus = VmCpus::<HyperCraftHalImpl, X64VcpuDevices<HyperCraftHalImpl>>::new();
    vcpus.add_vcpu(VCpu::new(0, p.vmcs_revision_id(), 0x7c00, npt).unwrap());

    let mut vm = VM::<
        HyperCraftHalImpl,
        X64VcpuDevices<HyperCraftHalImpl>,
        X64VmDevices<HyperCraftHalImpl>,
    >::new(vcpus);
    vm.bind_vcpu(0);

    if hart_id == 0 {
        let (_, dev) = vm.get_vcpu_and_device(0).unwrap();
        *(dev.console.lock().backend()) = device::device_emu::MultiplexConsoleBackend::Primary;

        for v in 0..256 {
            libax::hv::set_host_irq_enabled(v, true);
        }
    }

    println!("Running guest...");
    println!("{:?}", vm.run_vcpu(0));

    p.hardware_disable().unwrap();

    panic!("done");
}
