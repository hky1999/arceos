use core::ops::Deref;
use core::sync::atomic::{AtomicU32, AtomicUsize, Ordering};

use hypercraft::{VCpu, VmCpus, VM};

use super::arch::new_vcpu;
#[cfg(target_arch = "x86_64")]
use super::device::{self, NimbosVmDevices, X64VcpuDevices, X64VmDevices};
use crate::GuestPageTable;
use alloc::sync::Arc;
use axhal::{current_cpu_id, hv::HyperCraftHalImpl};

use crate::config::entry::vm_cfg_entry;
use crate::device::BarAllocImpl;

use hashbrown::HashMap;
use spin::Mutex;
use lazy_static::lazy_static;

lazy_static! {
    pub static ref VCPU_TO_PCPU: Mutex<HashMap<(u32, u32), u32>>  =
        Mutex::new(HashMap::new());
}

static VM_CNT: AtomicU32 = AtomicU32::new(0);

// use super::type1_5::cell;
static INIT_GPM_OK: AtomicU32 = AtomicU32::new(0);
static INITED_CPUS: AtomicUsize = AtomicUsize::new(0);

pub fn vcpu2pcpu(vm_id: u32, vcpu_id: u32) -> Option<u32> {
    let lock = VCPU_TO_PCPU.lock();
    lock.get(&(vm_id, vcpu_id)).cloned()
}

pub fn map_vcpu2pcpu(vm_id: u32, vcpu_id: u32, pcup_id: u32) {
    let mut lock = VCPU_TO_PCPU.lock();
    lock.insert((vm_id, vcpu_id), pcup_id);
}

pub fn config_boot_linux() {
    let hart_id = current_cpu_id();
    let linux_context = axhal::hv::get_linux_context();

    crate::arch::cpu_hv_hardware_enable(hart_id, linux_context)
        .expect("cpu_hv_hardware_enable failed");

    if hart_id == 0 {
        super::config::init_root_gpm().expect("init_root_gpm failed");
        INIT_GPM_OK.store(1, Ordering::Release);
    } else {
        while INIT_GPM_OK.load(Ordering::Acquire) < 1 {
            core::hint::spin_loop();
        }
    }

    let ept = super::config::root_gpm().nest_page_table();
    let ept_root = super::config::root_gpm().nest_page_table_root();

    let vm_id = VM_CNT.load(Ordering::SeqCst);
    VM_CNT.fetch_add(1, Ordering::SeqCst);

    debug!("create vcpu {} for vm {}", hart_id, vm_id);
    let vcpu = new_vcpu(
        hart_id,
        crate::arch::cpu_vmcs_revision_id(),
        ept_root,
        &linux_context,
    )
    .unwrap();
    let mut vcpus =
        VmCpus::<HyperCraftHalImpl, X64VcpuDevices<HyperCraftHalImpl, BarAllocImpl>>::new();
    info!("CPU{} add vcpu to vm...", hart_id);
    vcpus.add_vcpu(vcpu).expect("add vcpu failed");

    map_vcpu2pcpu(vm_id, hart_id as u32, hart_id as u32);

    let mut vm = VM::<
        HyperCraftHalImpl,
        X64VcpuDevices<HyperCraftHalImpl, BarAllocImpl>,
        X64VmDevices<HyperCraftHalImpl, BarAllocImpl>,
        GuestPageTable,
    >::new(vcpus, Arc::new(ept), vm_id);
    // The bind_vcpu method should be decoupled with vm struct.
    vm.bind_vcpu(hart_id).expect("bind vcpu failed");

    INITED_CPUS.fetch_add(1, Ordering::SeqCst);
    while INITED_CPUS.load(Ordering::Acquire) < axconfig::SMP {
        core::hint::spin_loop();
    }

    debug!("CPU{} before run vcpu", hart_id);
    info!("{:?}", vm.run_type15_vcpu(hart_id, &linux_context));

    // disable hardware virtualization todo
}

pub fn boot_vm(vm_id: usize) {
    let hart_id = current_cpu_id();
    let vm_cfg_entry = match vm_cfg_entry(vm_id) {
        Some(entry) => entry,
        None => {
            warn!("VM {} not existed, boot vm failed", vm_id);
            return;
        }
    };

    info!(
        "boot_vm {} {:?} on core {}, guest entry {:#x}",
        vm_id,
        vm_cfg_entry.get_vm_type(),
        axhal::current_cpu_id(),
        vm_cfg_entry.get_vm_entry(),
    );

    let gpm = vm_cfg_entry
        .generate_guest_phys_memory_set()
        .expect("Failed to generate GPM");

    let gpm = gpm.read().deref();
    let npt = gpm.nest_page_table();
    let npt_root = gpm.nest_page_table_root();
    info!("{:#x?}", gpm);

    let vm_id = VM_CNT.load(Ordering::SeqCst);
    VM_CNT.fetch_add(1, Ordering::SeqCst);
    let vcpu_id = 0;
    debug!("create vcpu {} for vm {}", vcpu_id, vm_id);
    // Main scheduling item, managed by `axtask`
    let vcpu = VCpu::new(
        vcpu_id,
        crate::arch::cpu_vmcs_revision_id(),
        vm_cfg_entry.get_vm_entry(),
        npt_root,
    )
    .unwrap();
    let mut vcpus =
        VmCpus::<HyperCraftHalImpl, X64VcpuDevices<HyperCraftHalImpl, BarAllocImpl>>::new();
    vcpus.add_vcpu(vcpu).expect("add vcpu failed");

    map_vcpu2pcpu(vm_id, vcpu_id as u32, hart_id as u32);

    let mut vm = VM::<
        HyperCraftHalImpl,
        X64VcpuDevices<HyperCraftHalImpl, BarAllocImpl>,
        NimbosVmDevices<HyperCraftHalImpl, BarAllocImpl>,
        GuestPageTable,
    >::new(vcpus, Arc::new(npt), vm_id);
    // The bind_vcpu method should be decoupled with vm struct.
    vm.bind_vcpu(vcpu_id).expect("bind vcpu failed");

    info!("Running guest...");
    info!("{:?}", vm.run_vcpu(0));
}
