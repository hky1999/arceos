use x86::apic;

use crate::mem::{phys_to_virt, PhysAddr, PAGE_SIZE_4K};
use crate::time::{busy_wait, Duration};
use core::sync::atomic::Ordering;

use super::header::HvHeader;
use super::percpu::PerCpu;

const START_PAGE_IDX: u8 = 6;
const START_PAGE_COUNT: usize = 1;
const START_PAGE_PADDR: PhysAddr = PhysAddr::from(START_PAGE_IDX as usize * PAGE_SIZE_4K);

/// Starts the given secondary CPU with its boot stack.
pub fn start_secondary_cpu(_apic_id: usize, _stack_top: crate::mem::PhysAddr) {
    // No need
    // This step is completed by Linux.
    // super::ARCEOS_MAIN_INIT_OK.store(1, Ordering::Release);
}

pub fn continue_secondary_cpus() {
    super::VMM_MAIN_INIT_OK.store(1, Ordering::Release);
}

core::arch::global_asm!(
   include_str!("ap_start.S"),
   start_page_paddr = const START_PAGE_PADDR.as_usize(),
);

/// Starts the given secondary CPU with its boot stack.
#[allow(clippy::uninit_assumed_init)]
pub fn start_arceos_cpus() {
    extern "C" {
        fn ap_entry32();
        fn ap_start();
        fn ap_end();
    }
    const U64_PER_PAGE: usize = PAGE_SIZE_4K / 8;

    let start_page_ptr = phys_to_virt(START_PAGE_PADDR).as_mut_ptr() as *mut u64;
    unsafe {
        let start_page =
            core::slice::from_raw_parts_mut(start_page_ptr, U64_PER_PAGE * START_PAGE_COUNT);

        // Since start page located at START_PAGE_PADDR belongs to Linux's physical address space.
        // We construct a backup space for start page, after `start_ap`, we just copy the backup space back.
        let mut backup: [u64; U64_PER_PAGE * START_PAGE_COUNT] =
            core::mem::MaybeUninit::uninit().assume_init();
        backup.copy_from_slice(start_page);
        core::ptr::copy_nonoverlapping(
            ap_start as *const u64,
            start_page_ptr,
            (ap_end as usize - ap_start as usize) / 8,
        );

        //   start_page[U64_PER_PAGE - 2] = stack_top.as_usize() as u64; // stack_top
        start_page[U64_PER_PAGE - 1] = ap_entry32 as usize as _; // entry

        let max_cpus = super::header::HvHeader::get().max_cpus;
        let mut arceos_cpu_num = 0;

        for apic_id in 0..max_cpus {
            if PerCpu::cpu_is_booted(apic_id as usize) {
                continue;
            }
            let stack_top = PerCpu::from_id_mut(apic_id).stack_top();
            start_page[U64_PER_PAGE - 2] = stack_top as u64; // stack_top
            
            super::apic::start_ap(apic_id, START_PAGE_IDX);
            arceos_cpu_num += 1;
            // wait for max 100ms
            busy_wait(Duration::from_millis(100)); // 100ms
        }
        debug!("starting {} CPUs for ArceOS ", arceos_cpu_num);

        start_page.copy_from_slice(&backup);
    }
}
