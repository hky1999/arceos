use alloc::string::String;

use axhal::current_cpu_id;
use axhal::mem::{phys_to_virt, PhysAddr};
use hypercraft::{GuestPhysAddr, HostPhysAddr};

use crate::config::entry::{vm_cfg_add_vm_entry, vm_cfg_entry, VMCfgEntry, VmType};
use crate::Error;
use crate::{nmi::nmi_send_msg_by_core_id, nmi::NmiMessage, HyperCraftHal, Result, VCpu};
// use axhal::hv::HyperCraftHalImpl;

pub const HVC_SHADOW_PROCESS_INIT: usize = 0x53686477;
pub const HVC_SHADOW_PROCESS_PRCS: usize = 0x70726373;
pub const HVC_SHADOW_PROCESS_RDY: usize = 0x52647921;

pub const HVC_AXVM_CREATE_CFG: usize = 0x101;
pub const HVC_AXVM_LOAD_IMG: usize = 0x102;
pub const HVC_AXVM_BOOT: usize = 0x103;

// The struct used for parameter passing between the kernel module and ArceOS hypervisor.
// This struct should have the same memory layout as the `AxVMCreateArg` structure in ArceOS.
// See jailhouse-arceos/driver/axvm.h
#[derive(Debug)]
#[repr(C, packed)]
pub struct AxVMCreateArg {
    /// VM ID, set by ArceOS hypervisor.
    vm_id: usize,
    /// Reserved.
    vm_type: usize,
    /// VM cpu mask.
    cpu_mask: usize,
    /// VM entry point.
    vm_entry_point: GuestPhysAddr,

    /// BIOS image loaded target guest physical address.
    bios_load_gpa: GuestPhysAddr,
    /// Kernel image loaded target guest physical address.
    kernel_load_gpa: GuestPhysAddr,
    /// randisk image loaded target guest physical address.
    ramdisk_load_gpa: GuestPhysAddr,

    /// Physical load address of BIOS, set by ArceOS hypervisor.
    bios_load_hpa: HostPhysAddr,
    /// Physical load address of kernel image, set by ArceOS hypervisor.
    kernel_load_hpa: HostPhysAddr,
    /// Physical load address of ramdisk image, set by ArceOS hypervisor.
    ramdisk_load_hpa: HostPhysAddr,
}

pub fn handle_hvc<H: HyperCraftHal>(
    vcpu: &mut VCpu<H>,
    id: usize,
    args: (usize, usize, usize),
) -> Result<u32> {
    debug!(
        "hypercall_handler vcpu: {}, id: {:#x?}, args: {:#x?}, {:#x?}, {:#x?}",
        vcpu.vcpu_id(),
        id,
        args.0,
        args.1,
        args.2
    );

    match id {
        HVC_SHADOW_PROCESS_INIT => {
            axtask::notify_all_process();
        }
        HVC_AXVM_CREATE_CFG => {
            // Translate guest physical address of AxVMCreateArg into virtual address of hypervisor.
            let arg_gpa = args.0;
            let arg_hpa = crate::config::root_gpm().translate(arg_gpa)?;
            let arg_hva = phys_to_virt(PhysAddr::from(arg_hpa)).as_mut_ptr();

            let arg = unsafe { &mut *{ arg_hva as *mut AxVMCreateArg } };

            debug!("HVC_AXVM_CREATE_CFG get\n{:#x?}", arg);

            let _ = ax_hvc_create_vm(arg)?;
        }
        HVC_AXVM_LOAD_IMG => {
            warn!("HVC_AXVM_LOAD_IMG is combined with HVC_AXVM_CREATE_CFG currently");
            warn!("Just return");
        }
        HVC_AXVM_BOOT => {
            ax_hvc_boot_vm(args.0);
        }
        _ => {
            warn!("Unhandled hypercall {}. vcpu: {:#x?}", id, vcpu);
        }
    }
    // Ok(0)
    // to compatible with jailhouse hypervisor test
    Ok(id as u32)
    // Err(HyperError::NotSupported)
}

fn ax_hvc_create_vm(cfg: &mut AxVMCreateArg) -> Result<u32> {
    // These fields should be set by user, but now this is provided by hypervisor.
    // Todo: refactor these.
    match VmType::from(cfg.vm_type) {
        VmType::VmTNimbOS => {
            cfg.vm_entry_point = crate::config::NIMBOS_VM_ENTRY;
            cfg.bios_load_gpa = crate::config::NIMBOS_BIOS_LOAD_GPA;
            cfg.kernel_load_gpa = crate::config::NIMBOS_KERNEL_LOAD_GPA;
            // No ramdisk for Nimbos.
            cfg.ramdisk_load_gpa = 0;
        }
        VmType::VmTLinux => {
            cfg.vm_entry_point = crate::config::LINUX_VM_ENTRY;
            cfg.bios_load_gpa = crate::config::LINUX_BIOS_LOAD_GPA;
            cfg.kernel_load_gpa = crate::config::LINUX_KERNEL_LOAD_GPA;
            cfg.ramdisk_load_gpa = crate::config::LINUX_RAMDISK_LOAD_GPA;
        }
        _ => {
            warn!("Unsupported VM Type {}", cfg.vm_type as u64);
            return Err(Error::InvalidParam);
        }
    }

    let mut vm_cfg = VMCfgEntry::new(
        String::from("Guest VM"),
        VmType::from(cfg.vm_type),
        String::from("guest cmdline"),
        cfg.cpu_mask,
        cfg.kernel_load_gpa,
        cfg.vm_entry_point,
        cfg.bios_load_gpa,
        cfg.ramdisk_load_gpa,
    );

    let mm_setup_fn = match VmType::from(cfg.vm_type) {
        VmType::VmTNimbOS => crate::config::nimbos_cfg_def::nimbos_memory_regions_setup,
        VmType::VmTLinux => crate::config::linux_cfg_def::linux_memory_regions_setup,
        _ => {
            warn!("Unsupported VM Type {}", cfg.vm_type as u64);
            return Err(Error::InvalidParam);
        }
    };

    vm_cfg.memory_region_editor(mm_setup_fn);

    vm_cfg.set_up_memory_region()?;

    // These fields should be set by hypervisor and read by Linux kernel module.
    (cfg.bios_load_hpa, cfg.kernel_load_hpa, cfg.ramdisk_load_hpa) = vm_cfg.get_img_load_info();

    let vm_id = vm_cfg_add_vm_entry(vm_cfg)?;

    // This field should be set by hypervisor and read by Linux kernel module.
    cfg.vm_id = vm_id;

    Ok(vm_id as u32)
}

fn ax_hvc_boot_vm(vm_id: usize) {
    let vm_cfg = match vm_cfg_entry(vm_id) {
        Some(entry) => entry,
        None => {
            warn!("VM {} not existed, boot vm failed", vm_id);
            return;
        }
    };
    let cpuset = vm_cfg.get_cpu_set();
    let vm_type = vm_cfg.get_vm_type();

    info!("boot VM {} {:?} on cpuset {:#x}", vm_id, vm_type, cpuset);

    let num_bits = core::mem::size_of::<u32>() * 8;
    let msg = NmiMessage::BootVm(vm_id);
    for i in 0..num_bits {
        if cpuset & (1 << i) != 0 {
            nmi_send_msg_by_core_id(i, msg);
        }
    }
}
