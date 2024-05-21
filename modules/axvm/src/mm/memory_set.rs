use alloc::collections::BTreeMap;
use core::{
    clone,
    fmt::{Debug, Display, Formatter, Result},
};
use memory_addr::PAGE_SIZE_4K;

use hypercraft::{GuestPageTableTrait, GuestPhysAddr, HostPhysAddr, HyperCraftHal};

use page_table_entry::MappingFlags;

use crate::{Error, GuestPageTable, Result as HyperResult};

use axhal::hv::HyperCraftHalImpl;

pub const fn is_aligned(addr: usize) -> bool {
    (addr & (HyperCraftHalImpl::PAGE_SIZE - 1)) == 0
}

#[derive(Debug, Clone, Copy)]
enum Mapper {
    Offset(usize),
}

#[derive(Debug, Clone)]
pub struct GuestMemoryRegion {
    pub gpa: GuestPhysAddr,
    pub hpa: HostPhysAddr,
    pub size: usize,
    pub flags: MappingFlags,
}

impl GuestMemoryRegion {
    fn is_overlap_with(&self, other: &Self) -> bool {
        let s0 = self.gpa;
        let e0 = s0 + self.size;
        let s1 = other.gpa;
        let e1 = s1 + other.size;
        !(e0 <= s1 || e1 <= s0)
    }

    fn target(&self, gpa: GuestPhysAddr) -> HostPhysAddr {
        self.hpa.wrapping_add(gpa.wrapping_sub(self.gpa))
    }

    fn map_to(&self, npt: &mut GuestPageTable) -> HyperResult {
        let mut start = self.gpa;
        let end = start + self.size;
        while start < end {
            let target = self.target(start);
            npt.map(start, target, self.flags)?;
            start += HyperCraftHalImpl::PAGE_SIZE;
        }
        Ok(())
    }

    fn unmap_to(&self, npt: &mut GuestPageTable) -> HyperResult {
        let mut start = self.gpa;
        let end = start + self.size;
        while start < end {
            npt.unmap(start)?;
            start += HyperCraftHalImpl::PAGE_SIZE;
        }
        Ok(())
    }
}

impl Display for GuestMemoryRegion {
    fn fmt(&self, f: &mut Formatter) -> Result {
        write!(
            f,
            "GuestMemoryRegion: GPA: [{:#x?}], HPA: [{:#x?}] size {:#x}, flags {:?}",
            &(self.gpa..self.gpa + self.size),
            &(self.hpa..self.hpa + self.size),
            &self.size,
            &self.flags
        )?;
        Ok(())
    }
}

/*
#[derive(Clone, Copy)]
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
        let start_gpa = if is_aligned(start_gpa) {
            start_gpa
        } else {
            let new_start_gpa = memory_addr::align_down_4k(start_gpa);
            // warn!(
            //     "start_gpa {:#x} aligned down to {:#x}",
            //     start_gpa, new_start_gpa
            // );
            new_start_gpa
        };
        let start_hpa = if is_aligned(start_hpa) {
            start_hpa
        } else {
            let new_start_hpa = memory_addr::align_down_4k(start_hpa);
            // warn!(
            //     "start_hpa {:#x} aligned down to {:#x}",
            //     start_hpa, new_start_hpa
            // );
            new_start_hpa
        };
        let size = if is_aligned(size) {
            size
        } else {
            let new_size = memory_addr::align_up_4k(size);
            // warn!("size {:#x} aligned up to {:#x}", size, new_size);
            new_size
        };
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
        debug!(
            "GPM Mapped Region [{:#x}-{:#x}] {:?}",
            start, end, self.flags
        );
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

impl Display for MapRegion {
    fn fmt(&self, f: &mut Formatter) -> Result {
        write!(
            f,
            "[{:#x?}], size {:#x}, flags {:?}",
            &(self.start..self.start + self.size),
            &self.size,
            &self.flags
        )?;
        Ok(())
    }
}

impl From<GuestMemoryRegion> for MapRegion {
    fn from(r: GuestMemoryRegion) -> Self {
        Self::new_offset(r.gpa, r.hpa, r.size, r.flags)
    }
}
 */
pub struct GuestPhysMemorySet {
    regions: BTreeMap<GuestPhysAddr, GuestMemoryRegion>,
    npt: GuestPageTable,
}

impl GuestPhysMemorySet {
    pub fn new() -> HyperResult<Self> {
        Ok(Self {
            npt: (GuestPageTable::new()?),
            regions: BTreeMap::new(),
        })
    }

    pub fn nest_page_table(&self) -> GuestPageTable {
        self.npt.clone()
    }

    pub fn nest_page_table_root(&self) -> HostPhysAddr {
        self.npt.root_paddr().into()
    }

    fn test_free_area(&self, other: &GuestMemoryRegion) -> bool {
        if let Some((_, before)) = self.regions.range(..other.gpa).last() {
            if before.is_overlap_with(other) {
                return false;
            }
        }
        if let Some((_, after)) = self.regions.range(other.gpa..).next() {
            if after.is_overlap_with(other) {
                return false;
            }
        }
        true
    }

    pub fn map_region(&mut self, region: GuestMemoryRegion) -> HyperResult {
        let mut mapped_region = region;
        debug!(
            "GPM Mapping Region [{:#x}-{:#x}] {:?}",
            region.start,
            region.start + region.size,
            region.flags
        );
        // TODO: determine why this part exists and should we keep the next part
        while mapped_region.size != 0 {
            if !self.test_free_area(&mapped_region) {
                // warn!(
                //     "MapRegion({:#x}..{:#x}) overlapped in:\n{:#x?}",
                //     region.start,
                //     region.start + region.size,
                //     self
                // );
                mapped_region.start += PAGE_SIZE_4K;
                mapped_region.size -= PAGE_SIZE_4K;
                // return Err(Error::InvalidParam);
            } else {
                break;
            }
        }

        if mapped_region.size == 0 {
            // debug!(
            //     "MapRegion({:#x}..{:#x}) is mapped or zero, just return",
            //     region.start,
            //     region.start + region.size
            // );
            return Ok(());
        }
        // TODO: determine why the previous part exists and should we keep this part
        if !self.test_free_area(&region) {
            warn!(
                "MapRegion({:#x}..{:#x}) overlapped in:\n{:#x?}",
                region.gpa,
                region.gpa + region.size,
                self
            );
            return Err(Error::InvalidParam);
        }
        region.map_to(&mut self.npt)?;
        self.regions.insert(region.gpa, region);
        Ok(())
    }

    pub fn clear(&mut self) {
        for region in self.regions.values() {
            region.unmap_to(&mut self.npt).unwrap();
        }
        self.regions.clear();
    }

    pub fn translate(&self, gpa: GuestPhysAddr) -> HyperResult<HostPhysAddr> {
        self.translate_by_walk(gpa)
    }

    pub fn translate_by_walk(&self, gpa: GuestPhysAddr) -> HyperResult<HostPhysAddr> {
        self.npt.translate(gpa)
    }

    pub fn lookup_region(&self, gpa: GuestPhysAddr) -> HyperResult<&GuestMemoryRegion> {
        let candidate = self.regions.range(..=gpa).next_back();
        match candidate {
            Some((_, region)) => {
                if gpa < region.gpa + region.size {
                    Ok(region)
                } else {
                    Err(Error::NotFound)
                }
            }
            None => Err(Error::NotFound),
        }
    }

    pub fn translate_and_get_limit(&self, gpa: GuestPhysAddr) -> HyperResult<(HostPhysAddr, usize)> {
        let region = self.lookup_region(gpa)?;
        let hpa = region.target(gpa);
        let limit = region.gpa + region.size - gpa;
        Ok((hpa, limit))
    }
}

impl Drop for GuestPhysMemorySet {
    fn drop(&mut self) {
        self.clear();
    }
}

impl Debug for GuestPhysMemorySet {
    fn fmt(&self, f: &mut Formatter) -> Result {
        // f.debug_struct("GuestPhysMemorySet")
        //     .field("page_table_root", &self.nest_page_table_root())
        //     .field("regions", &self.regions)
        //     .finish()
        write!(
            f,
            "GuestPhysMemorySet: page_table_root [{:#x}]\n",
            &self.nest_page_table_root()
        )?;
        for (_addr, region) in &self.regions {
            write!(f, "\t{:?}\n", region)?;
        }
        Ok(())
    }
}
