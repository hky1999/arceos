use core::ptr::NonNull;

use tock_registers::interfaces::{Readable, Writeable};
use tock_registers::registers::{ReadOnly, ReadWrite, WriteOnly};
use tock_registers::{register_bitfields, register_structs};

use kspin::SpinNoIrq;

use smmuv3::{PagingHandler, SMMUv3};

use axalloc::global_allocator;

use crate::mem::{phys_to_virt, virt_to_phys, MemRegionFlags, PhysAddr, VirtAddr, PAGE_SIZE_4K};

const SMMU_BASE: PhysAddr = pa!(0x9050000);

/// Implementation of [`PagingHandler`], to provide physical memory manipulation to
/// the [smmuv3] crate.
pub struct PagingHandlerImpl;

impl PagingHandler for PagingHandlerImpl {
    fn alloc_pages(num_pages: usize) -> Option<PhysAddr> {
        global_allocator()
            .alloc_pages(num_pages, PAGE_SIZE_4K)
            .map(|vaddr| virt_to_phys(vaddr.into()))
            .ok()
    }

    fn dealloc_pages(paddr: PhysAddr, num_pages: usize) {
        global_allocator().dealloc_pages(phys_to_virt(paddr).as_usize(), num_pages)
    }

    #[inline]
    fn phys_to_virt(paddr: PhysAddr) -> VirtAddr {
        phys_to_virt(paddr)
    }
}

static SMMU_V3: SpinNoIrq<SMMUv3<PagingHandlerImpl>> =
    SpinNoIrq::new(SMMUv3::new(phys_to_virt(SMMU_BASE).as_mut_ptr()));

/// Initializes SMMU on the primary CPU.
pub(crate) fn init() {
    info!("Initialize SMMU {}...", SMMU_V3.lock().version());

    SMMU_V3.lock().init();
}
