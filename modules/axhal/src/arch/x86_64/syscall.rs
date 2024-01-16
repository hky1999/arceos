use core::arch::global_asm;

use x86_64::addr::VirtAddr;
use x86_64::registers::model_specific::{Efer, EferFlags, KernelGsBase, LStar, SFMask, Star};
use x86_64::registers::rflags::RFlags;

use super::context::TrapFrame;
use super::gdt::GdtStruct;
use crate::platform::TSS_KERNEL_RSP_OFFSET;
// use crate::syscall::syscall;

#[percpu::def_percpu]
static PERCPU_USER_RSP: usize = 0;

global_asm!(
    include_str!("syscall.S"),
    saved_user_rsp = sym __PERCPU_PERCPU_USER_RSP,
    saved_kernel_rsp_offset = const 0,
);

#[no_mangle]
fn x86_syscall_handler(tf: &mut TrapFrame) {
	debug!("x86_syscall_handler {:?}", tf);
    // tf.rax = syscall(tf, tf.rax as _, tf.rdi as _, tf.rsi as _, tf.rdx as _) as u64;
    tf.rax = 0;
}

pub fn init_percpu() {
    extern "C" {
        fn syscall_entry();
    }
    unsafe {
        LStar::write(VirtAddr::new(syscall_entry as usize as _));
        Star::write(
            GdtStruct::UCODE64_SELECTOR,
            GdtStruct::UDATA_SELECTOR,
            GdtStruct::KCODE64_SELECTOR,
            GdtStruct::KDATA_SELECTOR,
        )
        .unwrap();
        SFMask::write(
            RFlags::TRAP_FLAG
                | RFlags::INTERRUPT_FLAG
                | RFlags::DIRECTION_FLAG
                | RFlags::IOPL_LOW
                | RFlags::IOPL_HIGH
                | RFlags::NESTED_TASK
                | RFlags::ALIGNMENT_CHECK,
        ); // TF | IF | DF | IOPL | AC | NT (0x47700)
        Efer::update(|efer| *efer |= EferFlags::SYSTEM_CALL_EXTENSIONS);
        KernelGsBase::write(VirtAddr::new(0));
    }
}
