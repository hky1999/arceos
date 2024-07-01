use hypercraft::{VCpu, VmxInterruptionType};

use axhal::hv::HyperCraftHalImpl;
use axhal::current_cpu_id;

use crate::Result;

pub fn hypercall_handler(
    vcpu: &mut VCpu<HyperCraftHalImpl>,
    id: u32,
    args: (usize, usize, usize),
) -> Result<u32> {
    // debug!("hypercall #{id:#x?}, args: {args:#x?}");
    crate::hvc::handle_hvc(vcpu, id as usize, args)
}

pub fn nmi_handler(vcpu: &mut VCpu<HyperCraftHalImpl>) -> Result<u32> {
    use crate::nmi::{NmiMessage, CORE_NMI_LIST};
    let current_cpu_id = current_cpu_id();
    let current_core_id = axhal::cpu_id_to_core_id(current_cpu_id);
    let msg = CORE_NMI_LIST[current_core_id].lock().pop();
    match msg {
        Some(NmiMessage::BootVm(vm_id)) => {
            crate::vm::boot_vm(vm_id);
            Ok(0)
        }
        None => {
            warn!(
                "CPU [{}] (Processor [{}])NMI VM-Exit",
                current_cpu_id, current_core_id
            );
            let int_info = vcpu.interrupt_exit_info()?;
            warn!(
                "interrupt_exit_info:{:#x}\n{:#x?}\n{:#x?}",
                vcpu.raw_interrupt_exit_info()?,
                int_info,
                vcpu
            );

            if int_info.int_type == VmxInterruptionType::NMI {
                unsafe { core::arch::asm!("int 2") }
            } else {
                // Reinject the event straight away.
                debug!(
                    "reinject to VM on CPU {} Processor {}",
                    current_cpu_id, current_core_id
                );
                vcpu.queue_event(int_info.vector, int_info.err_code);
            }
            Ok(0)
        }
    }
}

pub fn external_interrupt_handler(vcpu: &mut VCpu<HyperCraftHalImpl>) -> Result {
    let int_info = vcpu.interrupt_exit_info()?;
    debug!("VM-exit: external interrupt: {:#x?}", int_info);

    if int_info.vector != 0xf0 {
        panic!("VM-exit: external interrupt: {:#x?}", int_info);
    }

    assert!(int_info.valid);

    crate::irq::dispatch_host_irq(int_info.vector as usize)
}
