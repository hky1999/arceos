use alloc::string::String;
use alloc::vec::Vec;
use core::ops::Deref;
use core::sync::atomic::{AtomicU32, AtomicUsize, Ordering};
use memory_addr::PAGE_SIZE_4K;

use hypercraft::{VCpu, VmCpus};

use super::arch::new_vcpu;
#[cfg(target_arch = "x86_64")]
use super::device::{self, GuestVMDevices, X64VcpuDevices, X64VmDevices};
use crate::GuestPageTable;
use alloc::sync::Arc;
use axhal::{current_cpu_id, hv::HyperCraftHalImpl};

use crate::config::entry::vm_cfg_entry;
use crate::device::BarAllocImpl;

use hashbrown::HashMap;
use lazy_static::lazy_static;
use spin::Mutex;

lazy_static! {
    pub static ref VCPU_TO_PCPU: Mutex<HashMap<(u32, u32), u32>> = Mutex::new(HashMap::new());
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
        // X64VmDevices<HyperCraftHalImpl, BarAllocImpl>,
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

    // let gpm = gpm.read().deref();
    // let npt = gpm.nest_page_table();
    // let npt_root = gpm.nest_page_table_root();

    let npt_root = gpm.read().nest_page_table_root();
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
        // GuestVMDevices<HyperCraftHalImpl, BarAllocImpl>,
        GuestPageTable,
    >::new(vcpus, gpm, vm_id);
    // The bind_vcpu method should be decoupled with vm struct.
    vm.bind_vcpu(vcpu_id).expect("bind vcpu failed");

    info!("Running guest...");
    info!("{:?}", vm.run_vcpu(0));
}

use bit_set::BitSet;
use iced_x86::{Decoder, DecoderOptions, Formatter, Instruction, MasmFormatter, OpKind};
use spin::RwLock;

use hypercraft::{
    GuestPageTableTrait, GuestPageWalkInfo, GuestPhysAddr, GuestVirtAddr, HostPhysAddr,
    HostVirtAddr, HyperCraftHal, HyperError, HyperResult, LinuxContext, PerCpuDevices,
    PerVmDevices, VmxExitReason,
};

use crate::mm::{AddressSpace, GuestPhysMemorySet};

const VM_EXIT_INSTR_LEN_VMCALL: u8 = 3;
/// VM define.
pub struct VM<H: HyperCraftHal, PD: PerCpuDevices<H>, G: GuestPageTableTrait> {
    vcpus: VmCpus<H, PD>,
    vcpu_bond: BitSet,
    device: GuestVMDevices<H, BarAllocImpl>,
    vm_id: u32,
    /// EPT
    pub ept: Arc<G>,
    pub sys_mem: Arc<AddressSpace>,
}

impl<H: HyperCraftHal, PD: PerCpuDevices<H>, G: GuestPageTableTrait> VM<H, PD, G> {
    /// Create a new [`VM`].
    pub fn new(vcpus: VmCpus<H, PD>, mem_set: Arc<RwLock<GuestPhysMemorySet>>, vm_id: u32) -> Self {
        Self {
            vcpus,
            vcpu_bond: BitSet::new(),
            device: GuestVMDevices::<HyperCraftHalImpl, BarAllocImpl>::new(vm_id, mem_set.clone())
                .unwrap(),
            vm_id,
            ept: Arc::new(mem_set.as_ref().read().nest_page_table().clone()),
            sys_mem: Arc::new(AddressSpace::new(mem_set)),
        }
    }

    /// Bind the specified [`VCpu`] to current physical processor.
    pub fn bind_vcpu(&mut self, vcpu_id: usize) -> HyperResult<(&mut VCpu<H>, &mut PD)> {
        if self.vcpu_bond.contains(vcpu_id) {
            Err(HyperError::InvalidParam)
        } else {
            match self.vcpus.get_vcpu_and_device(vcpu_id) {
                Ok((vcpu, device)) => {
                    self.vcpu_bond.insert(vcpu_id);
                    vcpu.bind_to_current_processor()?;
                    Ok((vcpu, device))
                }
                e @ Err(_) => e,
            }
        }
    }

    #[allow(unreachable_code)]
    /// Run a specified [`VCpu`] on current logical vcpu.
    pub fn run_vcpu(&mut self, vcpu_id: usize) -> HyperResult {
        let (vcpu, vcpu_device) = self.vcpus.get_vcpu_and_device(vcpu_id).unwrap();

        loop {
            if let Some(exit_info) = vcpu.run() {
                // we need to handle vm-exit this by ourselves

                if exit_info.exit_reason == VmxExitReason::VMCALL {
                    let regs = vcpu.regs();
                    trace!("{:#x?}", regs);
                    let id = regs.rax as u32;
                    let args = (regs.rdi as usize, regs.rsi as usize, regs.rdx as usize);

                    match vcpu_device.hypercall_handler(vcpu, id, args) {
                        Ok(result) => vcpu.regs_mut().rax = result as u64,
                        Err(e) => panic!("Hypercall failed: {e:?}, hypercall id: {id:#x}, args: {args:#x?}, vcpu: {vcpu:#x?}"),
                    }

                    vcpu.advance_rip(VM_EXIT_INSTR_LEN_VMCALL)?;
                } else if exit_info.exit_reason == VmxExitReason::EXCEPTION_NMI {
                    match vcpu_device.nmi_handler(vcpu) {
                        Ok(result) => vcpu.regs_mut().rax = result as u64,
                        Err(e) => panic!("nmi_handler failed: {e:?}"),
                    }
                } else {
                    let result = vcpu_device.vmexit_handler(vcpu, &exit_info).or_else(|| {
                        let guest_rip = exit_info.guest_rip;
                        let length = exit_info.exit_instruction_length;
                        let instr = Self::decode_instr(self.ept.clone(), vcpu, guest_rip, length)
                            .expect("decode instruction failed");
                        self.device.vmexit_handler(vcpu, &exit_info, Some(instr))
                    });

                    match result {
                        Some(result) => {
                            if result.is_err() {
                                panic!(
                                    "VM failed to handle a vm-exit: {:?}, error {:?}, vcpu: {:#x?}",
                                    exit_info.exit_reason,
                                    result.unwrap_err(),
                                    vcpu
                                );
                            }
                        }
                        None => {
                            if exit_info.exit_reason == VmxExitReason::IO_INSTRUCTION {
                                let io_info = vcpu.io_exit_info().unwrap();
                                panic!(
                                    "nobody wants to handle this vm-exit: {:#x?}, io-info: {:?}",
                                    exit_info, io_info
                                );
                            } else {
                                panic!(
                                    "nobody wants to handle this vm-exit: {:#x?}, vcpu: {:#x?}",
                                    exit_info, vcpu
                                );
                            }
                        }
                    }
                }
            }

            vcpu_device.check_events(vcpu)?;
        }

        Ok(())
    }

    #[cfg(feature = "type1_5")]
    #[allow(unreachable_code)]
    /// Run a specified [`VCpu`] on current logical vcpu.
    pub fn run_type15_vcpu(&mut self, vcpu_id: usize, linux: &LinuxContext) -> HyperResult {
        let (vcpu, vcpu_device) = self.vcpus.get_vcpu_and_device(vcpu_id).unwrap();
        loop {
            if let Some(exit_info) = vcpu.run_type15(linux) {
                if exit_info.exit_reason == VmxExitReason::VMCALL {
                    let regs = vcpu.regs();
                    let id = regs.rax as u32;
                    let args = (regs.rdi as usize, regs.rsi as usize, regs.rdx as usize);

                    trace!("{:#x?}", regs);
                    match vcpu_device.hypercall_handler(vcpu, id, args) {
                        Ok(result) => vcpu.regs_mut().rax = result as u64,
                        Err(e) => panic!("Hypercall failed: {e:?}, hypercall id: {id:#x}, args: {args:#x?}, vcpu: {vcpu:#x?}"),
                    }

                    vcpu.advance_rip(VM_EXIT_INSTR_LEN_VMCALL)?;
                } else if exit_info.exit_reason == VmxExitReason::EXCEPTION_NMI {
                    match vcpu_device.nmi_handler(vcpu) {
                        Ok(result) => vcpu.regs_mut().rax = result as u64,
                        Err(e) => panic!("nmi_handler failed: {e:?}"),
                    }
                } else {
                    let result = vcpu_device.vmexit_handler(vcpu, &exit_info).or_else(|| {
                        let guest_rip = exit_info.guest_rip;
                        let length = exit_info.exit_instruction_length;
                        let instr = Self::decode_instr(self.ept.clone(), vcpu, guest_rip, length)
                            .expect("decode instruction failed");
                        self.device.vmexit_handler(vcpu, &exit_info, Some(instr))
                    });
                    debug!("this is result {:?}", result);
                    match result {
                        Some(result) => {
                            if result.is_err() {
                                panic!(
                                    "VM failed to handle a vm-exit: {:#x?}, error {:?}, vcpu: {:#x?}",
                                    exit_info.exit_reason,
                                    result.unwrap_err(),
                                    vcpu
                                );
                            }
                        }
                        None => {
                            panic!(
                                "nobody wants to handle this vm-exit: {:#x?}, vcpu: {:#x?}",
                                exit_info, vcpu
                            );
                        }
                    }
                }
                // debug!("test decode instruction");
                // let guest_rip = exit_info.guest_rip;
                // let length = exit_info.exit_instruction_length;
                // let instr = Self::decode_instr(self.ept.clone(), vcpu, guest_rip, length)?;
                // debug!("this is instr {:?}", instr);
            }
            // vcpu_device.check_events(vcpu)?;
        }
    }

    /// Unbind the specified [`VCpu`] bond by [`VM::<H>::bind_vcpu`].
    pub fn unbind_vcpu(&mut self, vcpu_id: usize) -> HyperResult {
        if self.vcpu_bond.contains(vcpu_id) {
            match self.vcpus.get_vcpu_and_device(vcpu_id) {
                Ok((vcpu, _)) => {
                    self.vcpu_bond.remove(vcpu_id);
                    vcpu.unbind_from_current_processor()?;
                    Ok(())
                }
                Err(e) => Err(e),
            }
        } else {
            Err(HyperError::InvalidParam)
        }
    }

    /// Get per-vm devices.
    pub fn devices(&mut self) -> &mut GuestVMDevices<HyperCraftHalImpl, BarAllocImpl> {
        &mut self.device
    }

    /// Get vcpu and its devices by its id.
    pub fn get_vcpu_and_device(&mut self, vcpu_id: usize) -> HyperResult<(&mut VCpu<H>, &mut PD)> {
        self.vcpus.get_vcpu_and_device(vcpu_id)
    }

    /// decode guest instruction
    pub fn decode_instr(
        ept: Arc<G>,
        vcpu: &VCpu<H>,
        guest_rip: usize,
        length: u32,
    ) -> HyperResult<Instruction> {
        let asm = Self::get_gva_content_bytes(ept, guest_rip, length, vcpu)?;
        let asm_slice = asm.as_slice();
        // Only one isntruction
        let mut decoder = Decoder::with_ip(64, asm_slice, guest_rip as u64, DecoderOptions::NONE);
        let instr = decoder.decode();
        // print instruction
        let mut output = String::new();
        let mut formatter = MasmFormatter::new();
        formatter.format(&instr, &mut output);
        // debug!("Instruction: {}", output);
        Ok(instr)
    }

    /// get gva content bytes
    pub fn get_gva_content_bytes(
        ept: Arc<G>,
        guest_rip: usize,
        length: u32,
        vcpu: &VCpu<H>,
    ) -> HyperResult<Vec<u8>> {
        // debug!(
        //     "get_gva_content_bytes: guest_rip: {:#x}, length: {:#x}",
        //     guest_rip, length
        // );
        let gva = vcpu.gla2gva(guest_rip);
        // debug!("get_gva_content_bytes: gva: {:#x}", gva);
        let gpa = Self::gva2gpa(ept.clone(), vcpu, gva)?;
        // debug!("get_gva_content_bytes: gpa: {:#x}", gpa);
        let hva = Self::gpa2hva(ept.clone(), gpa)?;
        // debug!("get_gva_content_bytes: hva: {:#x}", hva);
        let mut content = Vec::with_capacity(length as usize);
        let code_ptr = hva as *const u8;
        unsafe {
            for i in 0..length {
                let value_ptr = code_ptr.offset(i as isize);
                content.push(value_ptr.read());
            }
        }
        // debug!("get_gva_content_bytes: content: {:?}", content);
        Ok(content)
    }

    fn gpa2hva(ept: Arc<G>, gpa: GuestPhysAddr) -> HyperResult<HostVirtAddr> {
        let hpa = Self::gpa2hpa(ept, gpa)?;
        let hva = H::phys_to_virt(hpa);
        Ok(hva as HostVirtAddr)
    }

    fn gpa2hpa(ept: Arc<G>, gpa: GuestPhysAddr) -> HyperResult<HostPhysAddr> {
        ept.translate(gpa)
    }

    fn gva2gpa(ept: Arc<G>, vcpu: &VCpu<H>, gva: GuestVirtAddr) -> HyperResult<GuestPhysAddr> {
        let guest_ptw_info = vcpu.get_ptw_info();
        Self::page_table_walk(ept, guest_ptw_info, gva)
    }

    // suppose it is 4-level page table
    fn page_table_walk(
        ept: Arc<G>,
        pw_info: GuestPageWalkInfo,
        gva: GuestVirtAddr,
    ) -> HyperResult<GuestPhysAddr> {
        use x86_64::structures::paging::page_table::PageTableFlags as PTF;

        // debug!("page_table_walk: gva: {:#x}\npw_info:{:#x?}", gva, pw_info);
        const PHYS_ADDR_MASK: usize = 0x000f_ffff_ffff_f000; // bits 12..52
        if pw_info.level <= 1 {
            return Ok(gva as GuestPhysAddr);
        }
        let mut addr = pw_info.top_entry;
        let mut current_level = pw_info.level;
        let mut shift = 0;
        let mut page_size = PAGE_SIZE_4K;

        let mut entry = 0;

        while current_level != 0 {
            current_level -= 1;
            // get page table base addr
            addr = addr & PHYS_ADDR_MASK;

            let base = Self::gpa2hva(ept.clone(), addr)?;
            shift = (current_level * pw_info.width as usize) + 12;

            let index = (gva >> shift) & ((1 << (pw_info.width as usize)) - 1);
            page_size = 1 << shift;

            // get page table entry pointer
            let entry_ptr = unsafe { (base as *const usize).offset(index as isize) };

            // next page table addr (gpa)
            entry = unsafe { *entry_ptr };

            let entry_flags = PTF::from_bits_retain(entry as u64);

            // debug!("next page table entry {:#x} {:?}", entry, entry_flags);

            /* check if the entry present */
            if !entry_flags.contains(PTF::PRESENT) {
                warn!(
                    "GVA {:#x} l{} entry {:#x} not presented in its NPT",
                    gva,
                    current_level + 1,
                    entry
                );
                return Err(HyperError::BadState);
            }

            // Check hugepage
            if pw_info.pse && current_level > 0 && entry_flags.contains(PTF::HUGE_PAGE) {
                break;
            }

            addr = entry;
        }

        entry >>= shift;
        /* shift left 12bit more and back to clear XD/Prot Key/Ignored bits */
        entry <<= shift + 12;
        entry >>= 12;

        Ok((entry | (gva & (page_size - 1))) as GuestPhysAddr)
    }
}
