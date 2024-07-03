pub mod device_emu;

extern crate alloc;
use super::dummy_pci::DummyPciDevice;
use super::virtio::device::block::BlkDevConfig;
use super::virtio::{
    Block, DummyVirtioDevice, VirtioDevice, VirtioMsiIrqManager, VirtioPciDevice,
    GLOBAL_VIRTIO_PCI_CFG_REQ, VIRTIO_TYPE_BLOCK,
};
use crate::device::BarAllocImpl;
use crate::mm::AddressSpace;
use crate::{
    nmi::NmiMessage, nmi::CORE_NMI_LIST, HyperCraftHal, PerCpuDevices, PerVmDevices,
    Result as HyperResult, VCpu, VmExitInfo, VmxExitReason,
};
use crate::{Error as HyperError, GuestPageTable, VmExitInfo as VmxExitInfo};
use alloc::format;
use alloc::string::String;
use alloc::{sync::Arc, vec, vec::Vec};
use axhal::{current_cpu_id, mem::phys_to_virt};
use bit_field::BitField;
use core::any::Any;
use core::marker::PhantomData;
use core::ops::Range;
use core::sync::atomic::{AtomicU16, Ordering};
use device_emu::{ApicBaseMsrHandler, Bundle, VirtLocalApic};
use hypercraft::{GuestPageTableTrait, MmioOps, PioOps, VirtMsrOps, VmxInterruptionType};
use iced_x86::{Code, Instruction, OpKind, Register};
use page_table_entry::MappingFlags;
use pci::{AsAny, BarAllocTrait, PciDevOps, PciHost};
use spin::Mutex;
use x86_64::registers::rflags::RFlags;

use crate::config::{VMCfgEntry, VmType};

const VM_EXIT_INSTR_LEN_RDMSR: u8 = 2;
const VM_EXIT_INSTR_LEN_WRMSR: u8 = 2;
const VM_EXIT_INSTR_LEN_VMCALL: u8 = 3;

macro_rules! build_getcc {
    ($name:ident, $type:ty) => {
        fn $name(mut x: $type, y: $type) -> u64 {
            let result: u64;
            unsafe {
                core::arch::asm!("
                    sub {1}, {2}
                    pushfq
                    pop {0}",
                    out(reg) result,
                    inout(reg) x,
                    in(reg) y,
                    options(nostack)
                );
            }
            result
        }
    };
}

// build_getcc!(getcc8, u8);
build_getcc!(getcc16, u16);
build_getcc!(getcc32, u32);
build_getcc!(getcc64, u64);

// reg cannot use u8
fn getcc8(mut x: u8, y: u8) -> u64 {
    let result: u64;
    unsafe {
        core::arch::asm!("
            sub {1}, {2}
            pushfq
            pop {0}",
            out(reg) result,
            inout(reg_byte) x,
            in(reg_byte) y,
            options(nostack)
        );
    }
    result
}

fn getcc(access_size: u8, x: u64, y: u64) -> u64 {
    match access_size {
        1 => getcc8(x as u8, y as u8),
        2 => getcc16(x as u16, y as u16),
        4 => getcc32(x as u32, y as u32),
        _ => getcc64(x, y),
    }
}

pub struct DeviceList<H: HyperCraftHal, B: BarAllocTrait> {
    vm_id: usize,
    /// Emulated Memory I/O devices.
    memory_io_devices: Vec<Arc<Mutex<dyn MmioOps>>>,
    /// Emulated PCI devices.
    pci_devices: Option<Arc<Mutex<PciHost<B>>>>,

    /// Emulated MSR I/O devices (x86 specific).
    msr_devices: Vec<Arc<Mutex<dyn VirtMsrOps>>>,
    /// Emulated Port I/O devices (x86 specific).
    port_io_devices: Vec<Arc<Mutex<dyn PioOps>>>,
    /// A bundle for x86 related devices (x86 specific).
    bundle: Arc<Mutex<Bundle>>,
    /// Emulated I8259Pic devices (x86 specific).
    pic: [Arc<Mutex<device_emu::I8259Pic>>; 2],

    marker: core::marker::PhantomData<H>,
}

impl<H: HyperCraftHal, B: BarAllocTrait + 'static> DeviceList<H, B> {
    pub fn new(vm_id: usize) -> Self {
        let bundle = Arc::new(Mutex::new(Bundle::new()));
        let pic: [Arc<Mutex<device_emu::I8259Pic>>; 2] = [
            Arc::new(Mutex::new(device_emu::I8259Pic::new(0x20))),
            Arc::new(Mutex::new(device_emu::I8259Pic::new(0xA0))),
        ];
        Self {
            vm_id,
            port_io_devices: vec![],
            memory_io_devices: vec![],
            msr_devices: vec![],
            pci_devices: None,
            bundle,
            pic,
            marker: core::marker::PhantomData,
        }
    }

    /// Init devices for VM according to `VMCfgEntry`.
    pub fn init(&mut self, config: Arc<VMCfgEntry>) {
        /* Todo: Dynamically resolve devices from configuration files. */

        match config.get_vm_type() {
            VmType::VMTHostVM => {
                // Since we passthrough almost all devices to Host VM now.
                // Do nothing for Host VM.
            }
            VmType::VmTLinux | VmType::VmTNimbOS => {
                self.init_pci_host();
                let sys_mem = Arc::new(AddressSpace::new(
                    config.get_guest_phys_memory_set().unwrap(),
                ));
                let blk = Block::new(BlkDevConfig::default());
                self.add_virtio_pci_device(
                    String::from("virtio_blk"),
                    0x18,
                    Arc::new(Mutex::new(blk)),
                    sys_mem,
                )
                .unwrap();

                // PIC1: 0x20, 0x20 + 2
                self.add_port_io_device(self.pic[0].clone());
                // PIC2: 0xa0, 0xa0 + 2
                self.add_port_io_device(self.pic[1].clone());
                // Debug Port: 0x80, 0x80 + 1
                // self.add_port_io_device(Arc::new(Mutex::new(device_emu::DebugPort::new(0x80))));
                /*
                the complexity:
                - port 0x70 and 0x71 is for CMOS, but bit 7 of 0x70 is for NMI
                - port 0x40 ~ 0x43 is for PIT, but port 0x61 is also related
                */
                // SYS_CTRL_A: 0x92, 0x92 + 1
                self.add_port_io_device(Arc::new(Mutex::new(Bundle::proxy_system_control_a(
                    &self.bundle,
                ))));
                // SYS_CTRL_B: 0x61, 0x61 + 1
                self.add_port_io_device(Arc::new(Mutex::new(Bundle::proxy_system_control_b(
                    &self.bundle,
                ))));
                // CMOS: 0x70, 0x70 + 2
                self.add_port_io_device(Arc::new(Mutex::new(Bundle::proxy_cmos(&self.bundle))));
                // PIT: 0x40, 0x40 + 4
                self.add_port_io_device(Arc::new(Mutex::new(Bundle::proxy_pit(&self.bundle))));
                // FPU: 0xf0, 0xf0 + 2
                // self.add_port_io_device(Arc::new(Mutex::new(device_emu::Dummy::new(0xf0, 2))));
                // VGA: 0x3d4, 0x3d4 + 2
                // self.add_port_io_device(Arc::new(Mutex::new(device_emu::Dummy::new(0x3d4, 2))));
                // DMA: 0x87, 0x87 + 1
                self.add_port_io_device(Arc::new(Mutex::new(device_emu::Dummy::new(0x87, 1))));
                // ps/2 controller: 0x60, 0x60 + 1
                self.add_port_io_device(Arc::new(Mutex::new(device_emu::Dummy::new(0x60, 1))));
                // 0x64, 0x64 + 1
                self.add_port_io_device(Arc::new(Mutex::new(device_emu::Dummy::new(0x64, 1))));

                // self.add_msr_device(Arc::new(Mutex::new(device_emu::ProxyLocalApic::new())));
                // self.add_msr_device(Arc::new(Mutex::new(ApicBaseMsrHandler {})));
                // // linux read this amd-related msr on my intel cpu for some unknown reason... make it happy
                // self.add_msr_device(Arc::new(Mutex::new(device_emu::MsrDummy::new(0xc0011029))));
                const IA32_UMWAIT_CONTROL: u32 = 0xe1;
                self.add_msr_device(Arc::new(Mutex::new(device_emu::MsrDummy::new(
                    IA32_UMWAIT_CONTROL,
                ))));
            }
            _ => {}
        }
    }

    fn init_pci_host(&mut self) {
        let pci_host = PciHost::new(Some(Arc::new(super::virtio::VirtioMsiIrqManager {
            vm_id: self.vm_id as u32,
        })));

        let pci_devices = Arc::new(Mutex::new(pci_host));
        self.add_port_io_device(pci_devices.clone());
        self.pci_devices = Some(pci_devices);
    }

    fn add_pci_device(
        &mut self,
        name: String,
        dev_id: Arc<AtomicU16>,
        devfn: u8,
    ) -> HyperResult<()> {
        let mut pci_host = self.pci_devices.clone().unwrap();
        let pci_bus = pci_host.lock().root_bus.clone();
        let parent_bus = Arc::downgrade(&pci_bus);
        let mut pcidev = DummyPciDevice::<B>::new(name, devfn, parent_bus, 0x1010);
        pcidev.realize()
    }

    // virtio pci devfn: 0x18 bus: 0x0.
    fn add_virtio_pci_device(
        &mut self,
        name: String,
        devfn: u8,
        device: Arc<Mutex<dyn VirtioDevice>>,
        sys_mem: Arc<AddressSpace>,
    ) -> HyperResult<()> {
        let mut pci_host = self.pci_devices.clone().unwrap();
        let pci_bus = pci_host.lock().root_bus.clone();
        let parent_bus = Arc::downgrade(&pci_bus);
        let mut pcidev = VirtioPciDevice::<B>::new(name, devfn, sys_mem, device, parent_bus);
        pcidev.realize()
    }

    pub fn add_port_io_device(&mut self, device: Arc<Mutex<dyn PioOps>>) {
        self.port_io_devices.push(device)
    }

    pub fn add_port_io_devices(&mut self, devices: &mut Vec<Arc<Mutex<dyn PioOps>>>) {
        self.port_io_devices.append(devices)
    }

    pub fn find_port_io_device(&self, port: u16) -> Option<Arc<Mutex<dyn PioOps>>> {
        self.port_io_devices
            .iter()
            .find(|dev| dev.lock().port_range().contains(&port))
            .cloned()
            .or_else(|| {
                if let Some(pci_host) = &self.pci_devices {
                    let root_bus = &pci_host.lock().root_bus;
                    root_bus.clone().lock().find_pio_bar(port)
                } else {
                    None
                }
            })
    }

    pub fn intercepted_port_ranges(&self) -> Vec<Range<u16>> {
        self.port_io_devices
            .iter()
            .map(|pio_device| pio_device.lock().port_range())
            .collect()
    }

    pub fn add_memory_io_device(&mut self, device: Arc<Mutex<dyn MmioOps>>) {
        self.memory_io_devices.push(device)
    }

    pub fn add_memory_io_devices(&mut self, devices: &mut Vec<Arc<Mutex<dyn MmioOps>>>) {
        self.memory_io_devices.append(devices)
    }

    pub fn find_memory_io_device(&self, address: u64) -> Option<Arc<Mutex<dyn MmioOps>>> {
        self.memory_io_devices
            .iter()
            .find(|dev| dev.lock().mmio_range().contains(&address))
            .cloned()
            .or_else(|| {
                if let Some(pci_host) = &self.pci_devices {
                    let root_bus = &pci_host.lock().root_bus;
                    root_bus.clone().lock().find_mmio_bar(address)
                } else {
                    None
                }
            })
    }

    pub fn add_msr_device(&mut self, device: Arc<Mutex<dyn VirtMsrOps>>) {
        self.msr_devices.push(device)
    }

    pub fn add_msr_devices(&mut self, devices: &mut Vec<Arc<Mutex<dyn VirtMsrOps>>>) {
        self.msr_devices.append(devices)
    }

    pub fn find_msr_device(&self, msr: u32) -> Option<Arc<Mutex<dyn VirtMsrOps>>> {
        self.msr_devices
            .iter()
            .find(|dev| dev.lock().msr_range().contains(&msr))
            .cloned()
    }

    pub fn intercepted_msr_ranges(&self) -> Vec<Range<u32>> {
        self.msr_devices
            .iter()
            .map(|msr_device| msr_device.lock().msr_range())
            .collect()
    }

    fn handle_io_instruction_to_device(
        vcpu: &mut VCpu<H>,
        exit_info: &VmxExitInfo,
        device: Arc<Mutex<dyn PioOps>>,
    ) -> HyperResult {
        let io_info = vcpu.io_exit_info().unwrap();
        trace!(
            "VM exit: I/O instruction @ {:#x}: {:#x?}",
            exit_info.guest_rip,
            io_info,
        );

        if io_info.is_string {
            error!("INS/OUTS instructions are not supported!");
            return Err(HyperError::NotSupported);
        }
        if io_info.is_repeat {
            error!("REP prefixed I/O instructions are not supported!");
            return Err(HyperError::NotSupported);
        }
        if io_info.is_in {
            let value = device.lock().read(io_info.port, io_info.access_size)?;
            let rax = &mut vcpu.regs_mut().rax;
            // SDM Vol. 1, Section 3.4.1.1:
            // * 32-bit operands generate a 32-bit result, zero-extended to a 64-bit result in the
            //   destination general-purpose register.
            // * 8-bit and 16-bit operands generate an 8-bit or 16-bit result. The upper 56 bits or
            //   48 bits (respectively) of the destination general-purpose register are not modified
            //   by the operation.
            match io_info.access_size {
                1 => *rax = (*rax & !0xff) | (value & 0xff) as u64,
                2 => *rax = (*rax & !0xffff) | (value & 0xffff) as u64,
                4 => *rax = value as u64,
                _ => unreachable!(),
            }
        } else {
            let rax = vcpu.regs().rax;
            let value = match io_info.access_size {
                1 => rax & 0xff,
                2 => rax & 0xffff,
                4 => rax,
                _ => unreachable!(),
            } as u32;
            device
                .lock()
                .write(io_info.port, io_info.access_size, value)?;
        }
        vcpu.advance_rip(exit_info.exit_instruction_length as _)?;
        Ok(())
    }

    pub fn handle_io_instruction(
        &self,
        vcpu: &mut VCpu<H>,
        exit_info: &VmxExitInfo,
    ) -> Option<HyperResult> {
        let io_info = vcpu.io_exit_info().unwrap();
        if let Some(dev) = self.find_port_io_device(io_info.port) {
            let mut ret = Some(Self::handle_io_instruction_to_device(vcpu, exit_info, dev));
            // deal with virtio pci cfg access cap
            let mmio_req = GLOBAL_VIRTIO_PCI_CFG_REQ.read().clone();
            if let Some(req) = mmio_req.as_ref() {
                // this mmio req can only be generated from pci config read(virtio pci cfg access cap), so do not check mmio_ops in the devicelist
                *GLOBAL_VIRTIO_PCI_CFG_REQ.write() = None;
                if self.pci_devices.is_some() {
                    let addr = req.addr;
                    let pci_host = self.pci_devices.clone().unwrap();
                    let mut root_bus = &pci_host.lock().root_bus;
                    if let Some(mmio_ops) = root_bus.clone().lock().find_mmio_bar(addr) {
                        let access_size = req.len;
                        if req.is_write {
                            let mut bytes = [0u8; 8];
                            bytes.copy_from_slice(&(req.data)[..8]);
                            let value = u64::from_le_bytes(bytes);
                            let ret = Some(mmio_ops.lock().write(addr, access_size, value));
                        } else {
                            let value = mmio_ops.lock().read(addr, access_size).ok()?;
                            let rax = &mut vcpu.regs_mut().rax;
                            match access_size {
                                1 => *rax = (*rax & !0xff) | (value & 0xff) as u64,
                                2 => *rax = (*rax & !0xffff) | (value & 0xffff) as u64,
                                4 => *rax = (*rax & !0xffff_ffff) | (value & 0xffff_ffff) as u64,
                                8 => *rax = value,
                                _ => unreachable!(),
                            }
                            ret = Some(Ok(()))
                        }
                    }
                }
            }
            return ret;
        } else {
            return None;
        }
    }

    fn handle_mmio_instruction_to_device(
        vcpu: &mut VCpu<H>,
        exit_info: &VmxExitInfo,
        device: Arc<Mutex<dyn MmioOps>>,
        instr: Option<Instruction>,
    ) -> HyperResult {
        if let Some(instr) = instr {
            if let ept_info = vcpu
                .nested_page_fault_info()
                .expect("Failed to get nested page fault info")
            {
                let fault_addr = ept_info.fault_guest_paddr as u64;
                let is_write = ept_info.access_flags.contains(MappingFlags::WRITE);
                let access_size =
                    get_access_size(instr.clone()).expect("Failed to get access size");

                if is_write {
                    // Handle write.

                    debug!(
                        "[handle_mmio_instruction_to_device] write instr {}",
                        instr.clone()
                    );
                    let value = decode_value_from_instr(instr.clone(), vcpu)?;
                    debug!("[handle_mmio_instruction_to_device] write value:{:#x} to fault addr:{:#x} access_size:{:#x}", value, fault_addr, access_size);
                    match instr.op_code().instruction_string().to_lowercase() {
                        s if s.contains("mov") => {
                            debug!(
                                "this is write mmio and instr: {} access_size {:#x}",
                                s, access_size
                            );
                            device.lock().write(fault_addr, access_size, value)?;
                        }
                        _ => {
                            error!("unrealized instruction: {}", instr);
                            return Err(HyperError::InstructionNotSupported);
                        }
                    };
                } else {
                    // Handle read.

                    debug!(
                        "[handle_mmio_instruction_to_device] read begin instr \"{}\"",
                        instr.clone()
                    );
                    let value = device.lock().read(fault_addr, access_size)?;
                    debug!("[handle_mmio_instruction_to_device] read from fault addr:{:#x} value:{:#x} access_size:{:#x}", fault_addr, value, access_size);

                    let op_code = instr.op_code();
                    match op_code.instruction_string().to_lowercase() {
                        s if s.contains("mov") => {
                            emulated_instr_read(value, access_size, instr, vcpu);
                        }
                        s if s.contains("test") => {
                            let value2 = decode_value_from_instr(instr.clone(), vcpu)?;
                            let result = match access_size {
                                1 => (value2 & value) & 0xff,
                                2 => (value2 & value) & 0xffff,
                                4 => (value2 & value) & 0xffff_ffff,
                                8 => value2 & value,
                                _ => unreachable!(),
                            };
                            /*
                             * OF and CF are cleared; the SF, ZF and PF flags are set
                             * according to the result; AF is undefined.
                             *
                             * The updated status flags are obtained by subtracting 0 from
                             * 'result'.
                             */
                            let mut rflags = getcc(access_size, result, 0);
                            debug!(
                                "value1:{:#x} value2:{:#x} rflags:{:#x}",
                                value, value2, rflags
                            );
                            // clear OF and CF
                            rflags = rflags
                                & !(RFlags::OVERFLOW_FLAG.bits())
                                & !(RFlags::CARRY_FLAG.bits());
                            // set mask for ZF, PF, SF, OF, CF
                            let mask = RFlags::ZERO_FLAG.bits()
                                | RFlags::PARITY_FLAG.bits()
                                | RFlags::SIGN_FLAG.bits()
                                | RFlags::OVERFLOW_FLAG.bits()
                                | RFlags::CARRY_FLAG.bits();
                            vcpu.set_guest_rflags(rflags as usize, mask as usize)?;
                        }
                        _ => {
                            error!("unrealized instruction:{:?}", op_code.instruction_string());
                            return Err(HyperError::InstructionNotSupported);
                        }
                    }
                }
                vcpu.advance_rip(exit_info.exit_instruction_length as _)?;
                debug!("===============");
                return Ok(());
            } else {
                panic!(
                    "VM exit: EPT violation with unknown fault info @ {:#x}, vcpu: {:#x?}",
                    exit_info.guest_rip, vcpu
                );
            }
        }
        Err(HyperError::InvalidInstruction)
    }

    pub fn handle_mmio_instruction(
        &self,
        vcpu: &mut VCpu<H>,
        exit_info: &VmxExitInfo,
        instr: Option<Instruction>,
    ) -> Option<HyperResult> {
        match vcpu.nested_page_fault_info() {
            Ok(fault_info) => {
                warn!(
                    "VM exit: EPT violation @ {:#x}, fault_paddr={:#x}, access_flags=({:?})",
                    exit_info.guest_rip, fault_info.fault_guest_paddr, fault_info.access_flags
                );
                if let Some(dev) = self.find_memory_io_device(fault_info.fault_guest_paddr as u64) {
                    return Some(Self::handle_mmio_instruction_to_device(
                        vcpu, exit_info, dev, instr,
                    ));
                }
                warn!(
                    "VM exit Error: EPT violation @ {:#x}\nFault_paddr={:#x} access_flags=({:?}), vcpu: {:#x?}",
                    exit_info.guest_rip, fault_info.fault_guest_paddr, fault_info.access_flags, vcpu
                );
                return Some(Err(HyperError::InValidMmio));
            }
            Err(_err) => panic!(
                "VM exit: EPT violation with unknown fault info @ {:#x}, vcpu: {:#x?}",
                exit_info.guest_rip, vcpu
            ),
        }
        None
    }

    pub fn handle_msr_read(&self, vcpu: &mut VCpu<H>) -> HyperResult {
        let msr = vcpu.regs().rcx as u32;

        if let Some(dev) = self.find_msr_device(msr) {
            match dev.lock().read(msr) {
                Ok(value) => {
                    trace!("VM exit: RDMSR({:#x}) -> {:#x}", msr, value);

                    vcpu.regs_mut().rax = value & 0xffff_ffff;
                    vcpu.regs_mut().rdx = value >> 32;

                    vcpu.advance_rip(VM_EXIT_INSTR_LEN_RDMSR)?;
                    Ok(())
                }
                Err(e) => {
                    panic!("Failed to handle RDMSR({:#x}): {:?}", msr, e);
                }
            }
        } else {
            panic!("Unsupported RDMSR {:#x}, vcpu: {:#x?}", msr, vcpu);
        }
    }

    pub fn handle_msr_write(&self, vcpu: &mut VCpu<H>) -> HyperResult {
        let msr = vcpu.regs().rcx as u32;
        let value = (vcpu.regs().rax & 0xffff_ffff) | (vcpu.regs().rdx << 32);

        if let Some(dev) = self.find_msr_device(msr) {
            match dev.lock().write(msr, value) {
                Ok(_) => {
                    trace!("VM exit: WRMSR({:#x}) <- {:#x}", msr, value);

                    vcpu.advance_rip(VM_EXIT_INSTR_LEN_WRMSR)?;
                    Ok(())
                }
                Err(e) => {
                    panic!("Failed to handle WRMSR({:#x}): {:?}", msr, e);
                }
            }
        } else {
            panic!("Unsupported WRMSR {:#x}, vcpu: {:#x?}", msr, vcpu);
        }
    }
}

fn get_access_size(instruction: Instruction) -> HyperResult<u8> {
    // only consider
    match instruction.code() {
        Code::INVALID => Err(HyperError::DecodeError),
        _ => {
            // debug!("op0:{:?} op1:{:?}", instruction.op0_kind(), instruction.op1_kind());
            let size = match (instruction.op0_kind(), instruction.op1_kind()) {
                (OpKind::Register, _) => instruction.op_register(0).size(),
                (_, OpKind::Register) => instruction.op_register(1).size(),
                (OpKind::Immediate8, _) | (_, OpKind::Immediate8) => 1,
                (OpKind::Immediate16, _) | (_, OpKind::Immediate16) => 2,
                (OpKind::Immediate32, _) | (_, OpKind::Immediate32) => 4,
                (OpKind::Immediate64, _) | (_, OpKind::Immediate64) => 8,
                _ => 0,
            };
            Ok(size as u8)
        }
    }
}

// impl<H: HyperCraftHal, B: BarAllocTrait + 'static> GuestVMDevices<H, B> {
//     pub fn new(vm_id: u32, sys_mem: Arc<AddressSpace>) -> HyperResult<Self> {
//         let mut devices = DeviceList::new(vm_id as usize);
// init pci device
// devices.init_pci_host();
// devices.add_port_io_device(devices.pci_devices.clone().unwrap());
// This is just for test.
// devices.add_pci_device(String::from("pcitest"), Arc::new(AtomicU16::new(0)), 0x18)?;

// // Create a virtio dummy device
// let virtio_device_dummy = DummyVirtioDevice::new(VIRTIO_TYPE_BLOCK, 1, 4);
// devices.add_virtio_pci_device(
//     String::from("virtio_blk_dummy"),
//     0x18,
//     Arc::new(Mutex::new(virtio_device_dummy)),
//     sys_mem,
// )?;

// let blk = Block::new(BlkDevConfig::default());
// devices.add_virtio_pci_device(
//     String::from("virtio_blk"),
//     0x18,
//     Arc::new(Mutex::new(blk)),
//     sys_mem,
// )?;

//         Ok(Self {
//             marker: PhantomData,
//             devices,
//         })
//     }
// }

fn set_read_instr_value_of_specific_op_kind<H: HyperCraftHal>(
    value: u64,
    access_size: u8,
    op_kind: OpKind,
    reg: Register,
    vcpu: &mut VCpu<H>,
) -> bool {
    if op_kind != OpKind::Register {
        return false;
    }
    let gpr = match reg {
        Register::AX | Register::RAX | Register::EAX | Register::AL | Register::AH => {
            &mut vcpu.regs_mut().rax
        }
        Register::BX | Register::RBX | Register::EBX | Register::BL | Register::BH => {
            &mut vcpu.regs_mut().rbx
        }
        Register::CX | Register::RCX | Register::ECX | Register::CL | Register::CH => {
            &mut vcpu.regs_mut().rcx
        }
        Register::DX | Register::RDX | Register::EDX | Register::DL | Register::DH => {
            &mut vcpu.regs_mut().rdx
        }
        Register::SI | Register::RSI | Register::ESI | Register::SIL => &mut vcpu.regs_mut().rsi,
        Register::DI | Register::RDI | Register::EDI | Register::DIL => &mut vcpu.regs_mut().rdi,
        Register::BP | Register::RBP | Register::EBP | Register::BPL => &mut vcpu.regs_mut().rbp,
        Register::R8 | Register::R8D | Register::R8W | Register::R8L => &mut vcpu.regs_mut().r8,
        Register::R9 | Register::R9D | Register::R9W | Register::R9L => &mut vcpu.regs_mut().r9,
        Register::R10 | Register::R10D | Register::R10W | Register::R10L => {
            &mut vcpu.regs_mut().r10
        }
        Register::R11 | Register::R11D | Register::R11W | Register::R11L => {
            &mut vcpu.regs_mut().r11
        }
        Register::R12 | Register::R12D | Register::R12W | Register::R12L => {
            &mut vcpu.regs_mut().r12
        }
        Register::R13 | Register::R13D | Register::R13W | Register::R13L => {
            &mut vcpu.regs_mut().r13
        }
        Register::R14 | Register::R14D | Register::R14W | Register::R14L => {
            &mut vcpu.regs_mut().r14
        }
        Register::R15 | Register::R15D | Register::R15W | Register::R15L => {
            &mut vcpu.regs_mut().r15
        }
        _ => {
            return false;
        }
    };
    let ori = *gpr;

    // Clear GPR.
    *gpr = 0;

    match access_size {
        1 => *gpr = (*gpr & !0xff) | (value & 0xff) as u64,
        2 => *gpr = (*gpr & !0xffff) | (value & 0xffff) as u64,
        4 => *gpr = (*gpr & !0xffff_ffff) | (value & 0xffff_ffff) as u64,
        8 => *gpr = value,
        _ => {
            panic!("Invalid access_size {:#x}", access_size);
        }
    }

    debug!(
        "emulated_instr_read value {} set {:?} from {:#x} to {:#x}",
        value, reg, ori, *gpr
    );

    true
}

fn get_value_from_instr_of_specific_op_kind<H: HyperCraftHal>(
    instruction: Instruction,
    op_kind: OpKind,
    reg: Register,
    vcpu: &VCpu<H>,
) -> Option<u64> {
    match op_kind {
        OpKind::Register => match reg {
            Register::AX | Register::RAX | Register::EAX | Register::AL | Register::AH => {
                Some(vcpu.regs().rax)
            }
            Register::BX | Register::RBX | Register::EBX | Register::BL | Register::BH => {
                Some(vcpu.regs().rbx)
            }
            Register::CX | Register::RCX | Register::ECX | Register::CL | Register::CH => {
                Some(vcpu.regs().rcx)
            }
            Register::DX | Register::RDX | Register::EDX | Register::DL | Register::DH => {
                Some(vcpu.regs().rdx)
            }
            Register::SI | Register::RSI | Register::ESI | Register::SIL => Some(vcpu.regs().rsi),
            Register::DI | Register::RDI | Register::EDI | Register::DIL => Some(vcpu.regs().rdi),
            Register::BP | Register::RBP | Register::EBP | Register::BPL => Some(vcpu.regs().rbp),
            Register::R8 | Register::R8D | Register::R8W | Register::R8L => Some(vcpu.regs().r8),
            Register::R9 | Register::R9D | Register::R9W | Register::R9L => Some(vcpu.regs().r9),
            Register::R10 | Register::R10D | Register::R10W | Register::R10L => {
                Some(vcpu.regs().r10)
            }
            Register::R11 | Register::R11D | Register::R11W | Register::R11L => {
                Some(vcpu.regs().r11)
            }
            Register::R12 | Register::R12D | Register::R12W | Register::R12L => {
                Some(vcpu.regs().r12)
            }
            Register::R13 | Register::R13D | Register::R13W | Register::R13L => {
                Some(vcpu.regs().r13)
            }
            Register::R14 | Register::R14D | Register::R14W | Register::R14L => {
                Some(vcpu.regs().r14)
            }
            Register::R15 | Register::R15D | Register::R15W | Register::R15L => {
                Some(vcpu.regs().r15)
            }
            _ => {
                error!("Unsupported register {:?} of op kind {:?}", reg, op_kind);
                None
            }
        },
        OpKind::Immediate8 => Some(instruction.immediate8() as u64),
        OpKind::Immediate16 => Some(instruction.immediate16() as u64),
        OpKind::Immediate32 => Some(instruction.immediate32() as u64),
        OpKind::Immediate64 => Some(instruction.immediate64()),
        _ => None,
    }
}

fn decode_value_from_instr<H: HyperCraftHal>(
    instruction: Instruction,
    vcpu: &VCpu<H>,
) -> HyperResult<u64> {
    // only support 2 operands instruction
    if (instruction.op_count() > 2 || instruction.op_count() < 1) {
        error!(
            "Instruction {} can not be decoded, invalid op count {}",
            instruction,
            instruction.op_count()
        );
        return Err(HyperError::OperandNotSupported);
    }

    if let Some(value) = get_value_from_instr_of_specific_op_kind(
        instruction,
        instruction.op0_kind(),
        instruction.op0_register(),
        vcpu,
    ) {
        Ok(value)
    } else if let Some(value) = get_value_from_instr_of_specific_op_kind(
        instruction,
        instruction.op1_kind(),
        instruction.op1_register(),
        vcpu,
    ) {
        Ok(value)
    } else {
        error!("Instruction {} can not be decoded", instruction);
        Err(HyperError::OperandNotSupported)
    }
}

fn emulated_instr_read<H: HyperCraftHal>(
    value: u64,
    access_size: u8,
    instruction: Instruction,
    vcpu: &mut VCpu<H>,
) -> HyperResult {
    // only support 2 operands instruction
    if (instruction.op_count() > 2 || instruction.op_count() < 1) {
        error!(
            "Instruction {} can not be decoded, invalid op count {}",
            instruction,
            instruction.op_count()
        );
        return Err(HyperError::OperandNotSupported);
    }

    if set_read_instr_value_of_specific_op_kind(
        value,
        access_size,
        instruction.op0_kind(),
        instruction.op0_register(),
        vcpu,
    ) {
        return Ok(());
    } else if set_read_instr_value_of_specific_op_kind(
        value,
        access_size,
        instruction.op1_kind(),
        instruction.op1_register(),
        vcpu,
    ) {
        return Ok(());
    }
    Err(HyperError::OperandNotSupported)
}

fn get_instr_data(
    instruction: Instruction,
    is_write: bool,
) -> HyperResult<(OpKind, Option<String>)> {
    // only support 2 operands instruction
    let (kind, op_str) = match (instruction.op0_kind(), instruction.op1_kind()) {
        (OpKind::Register, _) => {
            let reg = instruction.op0_register();
            (OpKind::Register, Some(format!("{:?}", reg).to_lowercase()))
        }
        (_, OpKind::Register) => {
            let reg = instruction.op1_register();
            (OpKind::Register, Some(format!("{:?}", reg).to_lowercase()))
        }
        (OpKind::Immediate8, _) | (_, OpKind::Immediate8) => (
            OpKind::Immediate8,
            Some(format!("{:?}", instruction.immediate64())),
        ),
        (OpKind::Immediate16, _) | (_, OpKind::Immediate16) => (
            OpKind::Immediate16,
            Some(format!("{:?}", instruction.immediate64())),
        ),
        (OpKind::Immediate32, _) | (_, OpKind::Immediate32) => (
            OpKind::Immediate32,
            Some(format!("{:?}", instruction.immediate64())),
        ),
        (OpKind::Immediate64, _) | (_, OpKind::Immediate64) => (
            OpKind::Immediate64,
            Some(format!("{:?}", instruction.immediate64())),
        ),
        _ => return Err(HyperError::OperandNotSupported),
    };
    Ok((kind, op_str))
    // let op_code = instruction.op_code();
    // match op_code.instruction_string().to_lowercase() {
    //     s if s.contains("mov") => {
    //         debug!("this is instr: {}", s);
    //         return get_mov_data(instruction, is_write);
    //     }
    //     s if s.contains("test") => {
    //         debug!("this is instr: {}", s);
    //     }
    //     _ => {
    //         error!("unrealized instruction:{:?}", op_code.instruction_string());
    //         return Err(HyperError::InstructionNotSupported);
    //     }
    // };
    // Err(HyperError::InstructionNotSupported)
}

fn get_mov_data(instruction: Instruction, is_write: bool) -> HyperResult<(OpKind, Option<String>)> {
    // mov dest, src
    let op_kind = if is_write {
        instruction.op1_kind()
    } else {
        instruction.op0_kind()
    };
    match op_kind {
        OpKind::Immediate8 | OpKind::Immediate16 | OpKind::Immediate32 | OpKind::Immediate64 => {
            return Ok((op_kind, Some(format!("{:?}", instruction.immediate64()))));
        }
        OpKind::Register => {
            let reg = if is_write {
                instruction.op1_register()
            } else {
                instruction.op0_register()
            };
            return Ok((op_kind, Some(format!("{:?}", reg).to_lowercase())));
        }
        _ => {
            return Err(HyperError::OperandNotSupported);
        }
    };
    Err(HyperError::OperandNotSupported)
}
