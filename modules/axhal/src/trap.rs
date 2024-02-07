//! Trap handling.

use super::arch::TrapFrame;
use crate_interface::{call_interface, def_interface};
use memory_addr::VirtAddr;
use page_table_entry::MappingFlags;

/// Trap handler interface.
///
/// This trait is defined with the [`#[def_interface]`][1] attribute. Users
/// should implement it with [`#[impl_interface]`][2] in any other crate.
///
/// [1]: crate_interface::def_interface
/// [2]: crate_interface::impl_interface
#[def_interface]
pub trait TrapHandler {
    /// Handles interrupt requests for the given IRQ number.
    fn handle_irq(irq_num: usize);
    // more e.g.: handle_page_fault();
	/// Handle page fault from `axprocess`. Todo: redefine feature.
    #[cfg(feature = "monolithic")]
    fn handle_page_fault(addr: VirtAddr, flags: MappingFlags, tf: &mut TrapFrame);
}

#[cfg(feature = "monolithic")]
#[def_interface]
pub trait SyscallHandler {
	/// Handle syscall from user process.
    #[cfg(feature = "monolithic")]
    fn handle_syscall(syscall_id: usize, args: [usize; 6]) -> isize;
}

/// Call the external IRQ handler.
#[allow(dead_code)]
pub(crate) fn handle_irq_extern(irq_num: usize) {
    call_interface!(TrapHandler::handle_irq, irq_num);
}

#[allow(dead_code)]
#[cfg(feature = "monolithic")]
/// 分割token流
#[no_mangle]
pub(crate) fn handle_syscall(syscall_id: usize, args: [usize; 6]) -> isize {
    call_interface!(SyscallHandler::handle_syscall, syscall_id, args)
}

#[allow(dead_code)]
#[cfg(feature = "monolithic")]
pub(crate) fn handle_page_fault(addr: VirtAddr, flags: MappingFlags, tf: &mut TrapFrame) {
    call_interface!(TrapHandler::handle_page_fault, addr, flags, tf);
}