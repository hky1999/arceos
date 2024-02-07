/// Temporar module to boot Linux as a guest VM.
///
/// To be removed...
// use hypercraft::GuestPageTableTrait;
use alloc::string::String;


use hypercraft::{PerCpu, VCpu, VmCpus, VM};

use super::config;

#[cfg(target_arch = "x86_64")]
use super::device::{self, X64VcpuDevices, X64VmDevices};

use axtask::{current, AxTaskRef, TaskId, TaskInner, RUN_QUEUE};

use axhal::hv::HyperCraftHalImpl;

pub fn config_boot_first_vm(hart_id: usize) {
    info!("into main {}", hart_id);

    // Fix: this function shoule be moved to somewhere like vm_entry.
    // crate::arch::cpu_hv_hardware_enable(hart_id);

    // Alloc guest memory set.
    // Fix: this should be stored inside VM structure.
    let gpm: crate::mm::GuestPhysMemorySet = config::setup_gpm(hart_id).unwrap();
    let npt = gpm.nest_page_table_root();
    info!("{:#x?}", gpm);

    // Main scheduling item, managed by `axtask`
    let vcpu = VCpu::new(
        0,
        crate::arch::cpu_vmcs_revision_id(),
        config::BIOS_ENTRY,
        npt,
    )
    .unwrap();

    let mut vcpus = VmCpus::<HyperCraftHalImpl, X64VcpuDevices<HyperCraftHalImpl>>::new();
    // vcpus.add_vcpu(vcpu).expect("add vcpu failed");

    let mut vm = VM::<
        HyperCraftHalImpl,
        X64VcpuDevices<HyperCraftHalImpl>,
        X64VmDevices<HyperCraftHalImpl>,
    >::new(vcpus);

    let new_task = TaskInner::new_vcpu(
        String::from("vcpu"),
        axconfig::TASK_STACK_SIZE,
        0,
        0,
        vcpu,
        0,
    );

    // The bind_vcpu method should be decoupled with vm struct.
    // vm.bind_vcpu(0).expect("bind vcpu failed");

    if hart_id == 0 {
        let (_, dev) = vm.get_vcpu_and_device(0).unwrap();
        *(dev.console.lock().backend()) = device::device_emu::MultiplexConsoleBackend::Primary;

        for v in 0..256 {
            crate::irq::set_host_irq_enabled(v, true);
        }
    }

    // info!("Running guest...");
    // info!("{:?}", vm.run_vcpu(0));

    // crate::arch::cpu_hv_hardware_disable();

    panic!("done");
}
