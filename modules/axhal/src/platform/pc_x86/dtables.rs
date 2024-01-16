//! Description tables (per-CPU GDT, per-CPU ISS, IDT)
use crate::arch::{GdtStruct, IdtStruct, TaskStateSegment};
use crate::mem::VirtAddr;
use lazy_init::LazyInit;

use memoffset::offset_of;

static IDT: LazyInit<IdtStruct> = LazyInit::new();

#[percpu::def_percpu]
static TSS: LazyInit<TaskStateSegment> = LazyInit::new();

#[percpu::def_percpu]
static GDT: LazyInit<GdtStruct> = LazyInit::new();

fn init_percpu() {
    unsafe {
        IDT.load();
        let tss = TSS.current_ref_mut_raw();
        let gdt = GDT.current_ref_mut_raw();
        tss.init_by(TaskStateSegment::new());
        gdt.init_by(GdtStruct::new(tss));
        gdt.load();
        gdt.load_tss();
        crate::arch::syscall::init_percpu();
    }
}

/// Initializes IDT, GDT on the primary CPU.
pub(super) fn init_primary() {
    axlog::ax_println!("\nInitialize IDT & GDT...");
    IDT.init_by(IdtStruct::new());
    init_percpu();
}

/// Initializes IDT, GDT on secondary CPUs.
#[cfg(feature = "smp")]
pub(super) fn init_secondary() {
    init_percpu();
}

#[allow(dead_code)]
pub const TSS_KERNEL_RSP_OFFSET: usize = offset_of!(TaskStateSegment, privilege_stack_table);

pub fn kernel_stack_top() -> VirtAddr {
    unsafe {
        let tss = TSS.current_ref_mut_raw();
        VirtAddr::from(tss.privilege_stack_table[0].as_u64() as usize)
    }
}

pub fn set_kernel_stack_top(kstack_top: VirtAddr) {
    trace!("set percpu kernel stack: {:#x?}", kstack_top);
    unsafe {
        let tss = TSS.current_ref_mut_raw();
        tss.privilege_stack_table[0] = x86_64::VirtAddr::new(kstack_top.as_usize() as u64);
    }
}
