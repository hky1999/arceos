use alloc::string::{String, ToString};
use alloc::vec::Vec;

use axhal::current_cpu_id;
use axhal::mem::{phys_to_virt, PhysAddr};
use hypercraft::{GuestPhysAddr, HostPhysAddr};

use crate::config::args::{AxVMCreateArg, VmCreateCliArg};
use crate::config::{vm_cfg_add_vm_entry, vm_cfg_entry, VMCfgEntry, VmType};
use crate::mm::GuestMemoryRegion;
use crate::Error;
use crate::{nmi::nmi_send_msg_by_core_id, nmi::NmiMessage, HyperCraftHal, Result, VCpu};
// use axhal::hv::HyperCraftHalImpl;

pub const HVC_SHADOW_PROCESS_INIT: usize = 0x53686477;
pub const HVC_SHADOW_PROCESS_PRCS: usize = 0x70726373;
pub const HVC_SHADOW_PROCESS_RDY: usize = 0x52647921;

pub const HVC_AXVM_CREATE_CFG: usize = 0x101;
pub const HVC_AXVM_LOAD_IMG: usize = 0x102;
pub const HVC_AXVM_BOOT: usize = 0x103;

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

            // Awkward: In the process of initializing vcpu for the new VM, 
            // the `bind_to_current_processor` method of the target vcpu needs to be called, 
            // so `unbind_from_current_processor` is performed here 
            // to save the VMCS information of the current vcpu
            vcpu.unbind_from_current_processor()?;

            let _ = ax_hvc_create_vm(arg)?;
            
            vcpu.bind_to_current_processor()?;

            debug!("HVC_AXVM_CREATE_CFG create success");
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

use core::ffi::CStr;

fn ax_hvc_create_vm(cfg: &mut AxVMCreateArg) -> Result<u32> {
    debug!("ax_hvc_create_vm try to create VM [{}]", cfg.vm_id as u64);

    let raw_cfg_gpa = cfg.raw_cfg_base;

    let raw_cfg_hpa = crate::config::root_gpm().translate(raw_cfg_gpa)?;
    let raw_cfg_str =
        unsafe { CStr::from_ptr(phys_to_virt(PhysAddr::from(raw_cfg_hpa)).as_ptr() as *const _) }
            .to_string_lossy()
            .to_string();

    let vm_arg: VmCreateCliArg = toml::from_str(raw_cfg_str.as_str()).map_err(|err| {
        error!("toml deserialize get err {:?}", err);
        Error::InvalidParam
    })?;

    debug!("VM [{}] cfg: {:#x?}", cfg.vm_id as u64, vm_arg);

    let mut vm_cfg = VMCfgEntry::new(
        vm_arg.name,
        VmType::from(vm_arg.vm_type),
        String::from("guest cmdline"),
        vm_arg.cpu_set,
        vm_arg.kernel_load_addr,
        vm_arg.entry_point,
        vm_arg.bios_load_addr,
        vm_arg.ramdisk_load_addr.unwrap_or(0x0),
    );

    for mm_cfg in vm_arg.memory_regions {
        vm_cfg.append_memory_region(GuestMemoryRegion::from_config(mm_cfg)?)
    }

    // vm_cfg.set_up_memory_region()?;

    // // These fields should be set by hypervisor and read by Linux kernel module.
    // (cfg.bios_load_hpa, cfg.kernel_load_hpa, cfg.ramdisk_load_hpa) = vm_cfg.get_img_load_info();

    let vm_id = vm_cfg_add_vm_entry(vm_cfg)?;

    debug!("VM id from [{}] changed to [{}]", cfg.vm_id as u64, vm_id);

    let vm = crate::vm::setup_vm(vm_id)?;

    (cfg.bios_load_hpa, cfg.kernel_load_hpa, cfg.ramdisk_load_hpa) = vm.get_img_load_info()?;
    // // This field should be set by hypervisor and read by Linux kernel module.
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
