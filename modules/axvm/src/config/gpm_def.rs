use super::{BIOS_ENTRY, BIOS_PADDR, BIOS_SIZE, GUEST_PHYS_MEMORY_BASE, GUEST_PHYS_MEMORY_SIZE};
use crate::mm::{GuestMemoryRegion, GuestPhysMemorySet};
use crate::{phys_to_virt, virt_to_phys, Result as HyperResult};
use hypercraft::{
    GuestPageTableTrait, GuestPhysAddr, HostPhysAddr, HostVirtAddr,
};

use page_table_entry::MappingFlags;

#[repr(align(4096))]
pub(super) struct AlignedMemory<const LEN: usize>([u8; LEN]);

pub(super) static mut GUEST_PHYS_MEMORY: [AlignedMemory<GUEST_PHYS_MEMORY_SIZE>; 2] = [
    AlignedMemory([0; GUEST_PHYS_MEMORY_SIZE]),
    AlignedMemory([0; GUEST_PHYS_MEMORY_SIZE]),
];

fn gpa_as_mut_ptr(id: usize, guest_paddr: GuestPhysAddr) -> *mut u8 {
    let offset = unsafe { &(GUEST_PHYS_MEMORY[id]) as *const _ as usize };
    let host_vaddr = guest_paddr + offset;
    host_vaddr as *mut u8
}

#[cfg(target_arch = "x86_64")]
fn load_guest_image(id: usize, hpa: HostPhysAddr, load_gpa: GuestPhysAddr, size: usize) {
    let image_ptr = usize::from(phys_to_virt(hpa.into())) as *const u8;
    let image = unsafe { core::slice::from_raw_parts(image_ptr, size) };

    trace!(
        "loading to guest memory: host {:#x} to guest {:#x}, size {:#x}",
        image_ptr as usize,
        load_gpa,
        size
    );

    unsafe {
        core::slice::from_raw_parts_mut(gpa_as_mut_ptr(id, load_gpa), size).copy_from_slice(image)
    }
}

#[cfg(target_arch = "x86_64")]
pub fn setup_gpm(id: usize) -> HyperResult<GuestPhysMemorySet> {
    // copy BIOS and guest images

    load_guest_image(id, BIOS_PADDR, BIOS_ENTRY, BIOS_SIZE);
    #[cfg(feature = "guest_nimbos")]
    {
        load_guest_image(id, GUEST_IMAGE_PADDR, GUEST_ENTRY, GUEST_IMAGE_SIZE);
    }

    // create nested page table and add mapping
    let mut gpm = GuestPhysMemorySet::new()?;
    let guest_memory_regions = [
        GuestMemoryRegion {
            // Low RAM
            gpa: GUEST_PHYS_MEMORY_BASE,
            hpa: virt_to_phys((gpa_as_mut_ptr(id, GUEST_PHYS_MEMORY_BASE) as HostVirtAddr).into())
                .into(),
            size: GUEST_PHYS_MEMORY_SIZE,
            flags: MappingFlags::READ | MappingFlags::WRITE | MappingFlags::EXECUTE,
        },
        #[cfg(feature = "guest_linux")]
        GuestMemoryRegion {
            // Low RAM2
            gpa: 0x100_0000,
            hpa: 0x6100_0000,
            size: 0xf00_0000,
            flags: MappingFlags::READ | MappingFlags::WRITE | MappingFlags::EXECUTE,
        },
        #[cfg(feature = "guest_linux")]
        GuestMemoryRegion {
            // RAM
            gpa: 0x7000_0000,
            hpa: 0x7000_0000,
            size: 0x1000_0000,
            flags: MappingFlags::READ | MappingFlags::WRITE | MappingFlags::EXECUTE,
        },
        GuestMemoryRegion {
            // PCI
            gpa: 0x8000_0000,
            hpa: 0x8000_0000,
            size: 0x1000_0000,
            flags: MappingFlags::READ | MappingFlags::WRITE | MappingFlags::DEVICE,
        },
        GuestMemoryRegion {
            gpa: 0xfe00_0000,
            hpa: 0xfe00_0000,
            size: 0x1_0000,
            flags: MappingFlags::READ | MappingFlags::WRITE | MappingFlags::DEVICE,
        },
        GuestMemoryRegion {
            gpa: 0xfeb0_0000,
            hpa: 0xfeb0_0000,
            size: 0x10_0000,
            flags: MappingFlags::READ | MappingFlags::WRITE | MappingFlags::DEVICE,
        },
        GuestMemoryRegion {
            // IO APIC
            gpa: 0xfec0_0000,
            hpa: 0xfec0_0000,
            size: 0x1000,
            flags: MappingFlags::READ | MappingFlags::WRITE | MappingFlags::DEVICE,
        },
        GuestMemoryRegion {
            // HPET
            gpa: 0xfed0_0000,
            hpa: 0xfed0_0000,
            size: 0x1000,
            flags: MappingFlags::READ | MappingFlags::WRITE | MappingFlags::DEVICE,
        },
        GuestMemoryRegion {
            // Local APIC
            gpa: 0xfee0_0000,
            hpa: 0xfee0_0000,
            size: 0x1000,
            flags: MappingFlags::READ | MappingFlags::WRITE | MappingFlags::DEVICE,
        },
    ];
    for r in guest_memory_regions.into_iter() {
        gpm.map_region(r.into())?;
    }
    Ok(gpm)
}
