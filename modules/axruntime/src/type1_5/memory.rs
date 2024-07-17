use axhal::consts::{free_memory_start, hv_end};

use axhal::mem::{memory_regions, phys_to_virt, MemRegionFlags};
use axhal::paging::PageTable;
use memory_addr::VirtAddr;
// use page_table_entry::MappingFlags;

use spin::{Once, RwLock};

/// Page table used for hypervisor.
static HV_PT: Once<RwLock<PageTable>> = Once::new();

pub fn hv_page_table<'a>() -> &'a RwLock<PageTable> {
    HV_PT.get().expect("Uninitialized hypervisor page table!")
}

pub fn init_type15_allocator() {
    let mem_pool_start = free_memory_start();
    let mem_pool_end = hv_end().align_down_4k();

    let mem_pool_size = mem_pool_end.as_usize() - mem_pool_start.as_usize();
    info!(
        "global_init start:{:x}, end:{:x}.",
        mem_pool_start, mem_pool_end
    );
    axalloc::global_init(mem_pool_start.as_usize(), mem_pool_size);
}

pub fn activate_hv_pt() {
    let page_table = HV_PT.get().expect("Uninitialized hypervisor page table!");
    unsafe { axhal::arch::write_page_table_root(page_table.read().root_paddr()) };
}

pub fn init_hv_page_table() -> Result<(), axhal::paging::PagingError> {
    info!("Found physcial memory regions:");
    for r in memory_regions() {
        info!(
            "  [{:x?}, {:x?}) {} ({:?})",
            r.paddr,
            r.paddr + r.size,
            r.name,
            r.flags
        );
    }

    let mut page_table = PageTable::try_new().unwrap();

    info!("Generating HV memory mapping:...");

    for (i, r) in memory_regions().enumerate() {
        // The first two regions are memory belong to ArceOS itself.
        if i == 0 || i == 1 {
            info!(
                "Mapping  [{:x?}, {:x?}) {} ({:?})",
                r.paddr,
                r.paddr + r.size,
                r.name,
                r.flags
            );
            page_table.map_region(
                phys_to_virt(r.paddr),
                r.paddr,
                r.size,
                r.flags.into(),
                false,
            )?;
        } else {
            // The rest of memory regions belong to host Linux,
            // which are read from root cell config.
            let hv_virt_start = phys_to_virt(r.paddr);
            if hv_virt_start < VirtAddr::from(r.paddr.as_usize()) {
                let virt_start = r.paddr;
                panic!("Guest physical address {:#x} is too large", virt_start);
            }

            info!(
                "Mapping  [{:x?}, {:x?}) {} ({:?})",
                r.paddr,
                r.paddr + r.size,
                r.name,
                r.flags
            );
            // We just create mapping for all host memory regions.
            page_table.map_region(
                phys_to_virt(r.paddr),
                r.paddr,
                r.size,
                r.flags.into(),
                false,
            )?;
            // }
        }
    }
    info!("Hypervisor page table init end.");
    // info!("Hypervisor virtual memory set: {:#x?}", page_table);

    HV_PT.call_once(|| RwLock::new(page_table));
    Ok(())
}
