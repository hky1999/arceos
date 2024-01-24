use alloc::collections::BTreeMap;
use core::{
    fmt::{Debug, Formatter, Result},
    marker::PhantomData,
};

use hypercraft::{
    GuestPageTableTrait, GuestPhysAddr, HostPhysAddr, HostVirtAddr, HyperCraftHal, PerCpuDevices,
    PerVmDevices, VCpu, VmExitInfo,
};

use page_table_entry::MappingFlags;

use crate::{phys_to_virt, virt_to_phys, Error, GuestPageTable, Result as HyperResult};

use crate::hal::HyperCraftHalImpl;

pub const fn is_aligned(addr: usize) -> bool {
    (addr & (HyperCraftHalImpl::PAGE_SIZE - 1)) == 0
}

#[derive(Debug)]
enum Mapper {
    Offset(usize),
}

#[derive(Debug)]
pub struct GuestMemoryRegion {
    pub gpa: GuestPhysAddr,
    pub hpa: HostPhysAddr,
    pub size: usize,
    pub flags: MappingFlags,
}

pub struct MapRegion {
    pub start: GuestPhysAddr,
    pub size: usize,
    pub flags: MappingFlags,
    mapper: Mapper,
}

impl MapRegion {
    pub fn new_offset(
        start_gpa: GuestPhysAddr,
        start_hpa: HostPhysAddr,
        size: usize,
        flags: MappingFlags,
    ) -> Self {
        assert!(is_aligned(start_gpa));
        assert!(is_aligned(start_hpa));
        assert!(is_aligned(size));
        let offset = start_gpa - start_hpa;
        Self {
            start: start_gpa,
            size,
            flags,
            mapper: Mapper::Offset(offset),
        }
    }

    fn is_overlap_with(&self, other: &Self) -> bool {
        let s0 = self.start;
        let e0 = s0 + self.size;
        let s1 = other.start;
        let e1 = s1 + other.size;
        !(e0 <= s1 || e1 <= s0)
    }

    fn target(&self, gpa: GuestPhysAddr) -> HostPhysAddr {
        match self.mapper {
            Mapper::Offset(off) => gpa.wrapping_sub(off),
        }
    }

    fn map_to(&self, npt: &mut GuestPageTable) -> HyperResult {
        let mut start = self.start;
        let end = start + self.size;
        while start < end {
            let target = self.target(start);
            npt.map(start, target, self.flags)?;
            start += HyperCraftHalImpl::PAGE_SIZE;
        }
        Ok(())
    }

    fn unmap_to(&self, npt: &mut GuestPageTable) -> HyperResult {
        let mut start = self.start;
        let end = start + self.size;
        while start < end {
            npt.unmap(start)?;
            start += HyperCraftHalImpl::PAGE_SIZE;
        }
        Ok(())
    }
}

impl Debug for MapRegion {
    fn fmt(&self, f: &mut Formatter) -> Result {
        f.debug_struct("MapRegion")
            .field("range", &(self.start..self.start + self.size))
            .field("size", &self.size)
            .field("flags", &self.flags)
            .field("mapper", &self.mapper)
            .finish()
    }
}

impl From<GuestMemoryRegion> for MapRegion {
    fn from(r: GuestMemoryRegion) -> Self {
        Self::new_offset(r.gpa, r.hpa, r.size, r.flags)
    }
}

pub struct GuestPhysMemorySet {
    regions: BTreeMap<GuestPhysAddr, MapRegion>,
    npt: GuestPageTable,
}

impl GuestPhysMemorySet {
    pub fn new() -> HyperResult<Self> {
        Ok(Self {
            npt: GuestPageTable::new()?,
            regions: BTreeMap::new(),
        })
    }

    pub fn nest_page_table_root(&self) -> HostPhysAddr {
        self.npt.root_paddr().into()
    }

    fn test_free_area(&self, other: &MapRegion) -> bool {
        if let Some((_, before)) = self.regions.range(..other.start).last() {
            if before.is_overlap_with(other) {
                return false;
            }
        }
        if let Some((_, after)) = self.regions.range(other.start..).next() {
            if after.is_overlap_with(other) {
                return false;
            }
        }
        true
    }

    pub fn map_region(&mut self, region: MapRegion) -> HyperResult {
        if region.size == 0 {
            return Ok(());
        }
        if !self.test_free_area(&region) {
            warn!(
                "MapRegion({:#x}..{:#x}) overlapped in:\n{:#x?}",
                region.start,
                region.start + region.size,
                self
            );
            return Err(Error::InvalidParam);
        }
        region.map_to(&mut self.npt)?;
        self.regions.insert(region.start, region);
        Ok(())
    }

    pub fn clear(&mut self) {
        for region in self.regions.values() {
            region.unmap_to(&mut self.npt).unwrap();
        }
        self.regions.clear();
    }
}

impl Drop for GuestPhysMemorySet {
    fn drop(&mut self) {
        self.clear();
    }
}

impl Debug for GuestPhysMemorySet {
    fn fmt(&self, f: &mut Formatter) -> Result {
        f.debug_struct("GuestPhysMemorySet")
            .field("page_table_root", &self.nest_page_table_root())
            .field("regions", &self.regions)
            .finish()
    }
}

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
