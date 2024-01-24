use axalloc::global_allocator;
use axhal::mem::{phys_to_virt, virt_to_phys, PAGE_SIZE_4K};
use hypercraft::{HostPhysAddr, HostVirtAddr, HyperCraftHal};

/// An empty struct to implementate of `HyperCraftHal`
pub struct HyperCraftHalImpl;

impl HyperCraftHal for HyperCraftHalImpl {
    fn alloc_pages(num_pages: usize) -> Option<hypercraft::HostVirtAddr> {
        global_allocator()
            .alloc_pages(num_pages, PAGE_SIZE_4K)
            .map(|pa| pa as HostVirtAddr)
            .ok()
    }

    fn dealloc_pages(pa: HostVirtAddr, num_pages: usize) {
        global_allocator().dealloc_pages(pa as usize, num_pages);
    }

    fn phys_to_virt(pa: HostPhysAddr) -> HostVirtAddr {
        phys_to_virt(pa.into()).into()
    }

    fn virt_to_phys(va: HostVirtAddr) -> HostPhysAddr {
        virt_to_phys(va.into()).into()
    }

    fn current_time_nanos() -> u64 {
        axhal::time::current_time_nanos()
    }
}
