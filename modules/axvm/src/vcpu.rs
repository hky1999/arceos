use alloc::sync::{Arc, Weak};
use alloc::vec::Vec;
use anyhow::Ok;
use spin::{Lazy, Mutex};

use hypercraft::VCpu as ArchVCpu;
use hypercraft::VmxExitReason;
use hypercraft::{HyperError, HyperResult};

use axhal::{current_cpu_id, hv::HyperCraftHalImpl};

use crate::config::{VMCfgEntry, VmType};
use crate::vm::VM;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum VcpuState {
    Inv = 0,
    Runnable = 1,
    Running = 2,
    Blocked = 3,
}

#[derive(Clone)]
#[repr(transparent)]
pub struct Vcpu(pub Arc<VcpuInner>);

impl PartialEq for Vcpu {
    fn eq(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.0, &other.0)
    }
}

impl Eq for Vcpu {}

#[allow(dead_code)]
pub struct WeakVcpu(Weak<VcpuInner>);

#[allow(dead_code)]
impl WeakVcpu {
    pub fn upgrade(&self) -> Option<Vcpu> {
        self.0.upgrade().map(Vcpu)
    }
}

pub struct VcpuInner {
    inner_const: VcpuConst,
    pub inner_mut: Mutex<VcpuInnerMut>,
}

struct VcpuConst {
    id: usize,      // vcpu_id
    vm: Weak<VM>,   // weak pointer to related Vm
    phys_id: usize, // related physical CPU id
}

pub struct VcpuInnerMut {
    state: VcpuState,
    // int_list: Vec<usize>,
    // regs: ArchVcpuRegs
    arch_vcpu: ArchVCpu<HyperCraftHalImpl>,
}

impl VcpuInnerMut {
    fn new(vcpu_id: usize) -> Self {
        Self {
            state: VcpuState::Inv,
            // int_list: vec![],
            arch_vcpu: ArchVCpu::<HyperCraftHalImpl>::new(
                vcpu_id,
                crate::arch::cpu_vmcs_revision_id(),
            )
            .expect("Failed to construct vcpu"),
        }
    }
}

impl Vcpu {
    pub(super) fn new(vm: Weak<VM>, vcpu_id: usize, phys_id: usize, config: &VMCfgEntry) -> Self {
        debug!(
            "New VCpu[v:{},p:{}] for VM[{}]",
            vcpu_id,
            phys_id,
            config.vm_id()
        );
        let inner_const = VcpuConst {
            id: vcpu_id,
            vm,
            phys_id,
        };
        let inner = Arc::new(VcpuInner {
            inner_const,
            inner_mut: Mutex::new(VcpuInnerMut::new(vcpu_id)),
        });
        Self(inner)
    }

    pub fn init(&self, vm: &VM) -> HyperResult {
        debug!("Init VCpu [{}] for VM[{}]", self.id(), vm.id());
        let mut inner = self.0.inner_mut.lock();

        let ept_root = vm.nest_page_table_root();
        match vm.vm_type() {
            VmType::VMTHostVM => {
                inner.arch_vcpu.setup_from_host(
                    ept_root,
                    axhal::hv::get_linux_context(self.0.inner_const.phys_id),
                )?;
            }
            VmType::VmTLinux | VmType::VmTNimbOS => {
                inner
                    .arch_vcpu
                    .setup(ept_root, vm.config().get_vm_entry())?;
                for pio_range in vm.devices().intercepted_port_ranges() {
                    debug!("Intercept Ports {:#x?} for VM[{}]", pio_range, vm.id());
                    inner.arch_vcpu.set_io_intercept_of_range(
                        pio_range.start as u32,
                        pio_range.count() as u32,
                        true,
                    );
                }
                for msr_range in vm.devices().intercepted_msr_ranges() {
                    debug!("Intercept MSRs {:#x?} for VM[{}]", msr_range, vm.id());
                    for msr in msr_range {
                        inner.arch_vcpu.set_msr_intercept_of_range(msr, true);
                    }
                }
            }
            _ => {
                warn!("Illegal VM Type");
                return HyperResult::Err(HyperError::InvalidParam);
            }
        }
        HyperResult::Ok(())
    }

    pub fn vm(&self) -> Arc<VM> {
        self.0.inner_const.vm.upgrade().unwrap()
    }

    pub fn id(&self) -> usize {
        self.0.inner_const.id
    }

    pub fn run(&self) -> HyperResult {
        debug!("Vcpu [{}] running...", self.id());

        let mut inner = self.0.inner_mut.lock();
        let vcpu = &mut inner.arch_vcpu;
        vcpu.bind_to_current_processor()?;
        loop {
            if let Some(exit_info) = vcpu.run() {
                match exit_info.exit_reason {
                    VmxExitReason::VMCALL => {
                        const VM_EXIT_INSTR_LEN_VMCALL: u8 = 3;
                        let regs = vcpu.regs();
                        let id = regs.rax as u32;
                        let args = (regs.rdi as usize, regs.rsi as usize, regs.rdx as usize);

                        trace!("{:#x?}", regs);
                        match crate::handlers::hypercall_handler(vcpu, id, args) {
                            HyperResult::Ok(result) => vcpu.regs_mut().rax = result as u64,
                            Err(e) => panic!("Hypercall failed: {e:?}, hypercall id: {id:#x}, args: {args:#x?}, vcpu: {vcpu:#x?}"),
                        }

                        vcpu.advance_rip(VM_EXIT_INSTR_LEN_VMCALL)?;
                    }
                    VmxExitReason::EXCEPTION_NMI => match crate::handlers::nmi_handler(vcpu) {
                        HyperResult::Ok(result) => vcpu.regs_mut().rax = result as u64,
                        Err(e) => panic!("nmi_handler failed: {e:?}"),
                    },

                    VmxExitReason::EPT_VIOLATION => {
                        let guest_rip = exit_info.guest_rip;
                        let length = exit_info.exit_instruction_length;
                        let vm = self.vm();
                        let instr = vm
                            .decode_instr(
                                &vcpu, guest_rip, length,
                            )
                            .expect("decode instruction failed");
                        vm.devices()
                            .handle_mmio_instruction(vcpu, &exit_info, Some(instr))
                            .unwrap()?;
                    }
                    VmxExitReason::IO_INSTRUCTION => self
                        .vm()
                        .devices()
                        .handle_io_instruction(vcpu, &exit_info)
                        .unwrap()?,
                    VmxExitReason::MSR_READ => self.vm().devices().handle_msr_read(vcpu)?,
                    VmxExitReason::MSR_WRITE => self.vm().devices().handle_msr_write(vcpu)?,
                    _ => {
                        panic!(
                            "nobody wants to handle this vm-exit: {:#x?}, vcpu: {:#x?}",
                            exit_info, vcpu
                        );
                    }
                }
            }
        }
        HyperResult::Ok(())
    }
}
