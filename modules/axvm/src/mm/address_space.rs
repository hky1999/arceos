use alloc::sync::Arc;
use alloc::vec::Vec;
use spin::RwLock;

use axhal::mem::phys_to_virt;
use pci::util::byte_code::ByteCode;

use hypercraft::{
    GuestPhysAddr, HostPhysAddr, HostVirtAddr, HyperError, HyperResult as Result, VirtioError,
};

use crate::mm::iovec::Iovec;
use crate::mm::GuestPhysMemorySet;

/// A wrapper of GuestPhysMemorySet.
///
/// It also provides some helper functions to read/write data from/to guest/host memory.
pub struct AddressSpace {
    inner: Arc<RwLock<GuestPhysMemorySet>>,
}

impl AddressSpace {
    pub fn new(inner: GuestPhysMemorySet) -> Self {
        AddressSpace {
            inner: Arc::new(RwLock::new(inner)),
        }
    }

    pub fn translate(&self, addr: GuestPhysAddr) -> Result<HostPhysAddr> {
        let inner = self.inner.read();
        inner.translate(addr)
    }

    pub fn translate_and_get_limit(&self, addr: GuestPhysAddr) -> Result<(HostPhysAddr, usize)> {
        let inner = self.inner.read();
        inner.translate_and_get_limit(addr)
    }

    pub fn checked_offset_address(
        &self,
        base: GuestPhysAddr,
        offset: usize,
    ) -> Result<GuestPhysAddr> {
        let inner = self.inner.read();
        let limit = inner.get_limit(base)?;

        if offset >= limit {
            Err(HyperError::VirtioError(VirtioError::AddressOverflow(
                "Offset overflow",
                base,
                offset,
            )))
        } else {
            Ok(base.wrapping_add(offset))
        }
    }

    pub fn read_from_host_virt(&self, addr: HostVirtAddr, buf: &mut [u8]) -> Result<()> {
        unsafe {
            core::ptr::copy_nonoverlapping(addr as *const u8, buf.as_mut_ptr(), buf.len());
        }
        Ok(())
    }

    pub fn read_from_host(&self, addr: HostPhysAddr, buf: &mut [u8]) -> Result<()> {
        self.read_from_host_virt(phys_to_virt(addr.into()).as_usize(), buf)
    }

    pub fn read_object_from_host_virt<T: ByteCode>(&self, addr: HostVirtAddr) -> Result<T> {
        let mut obj = T::default();
        let buf = obj.as_mut_bytes();
        self.read_from_host_virt(addr, buf)?;
        Ok(obj)
    }

    pub fn read_object_from_host<T: ByteCode>(&self, addr: HostPhysAddr) -> Result<T> {
        let mut obj = T::default();
        let buf = obj.as_mut_bytes();
        self.read_from_host(addr, buf)?;
        Ok(obj)
    }

    pub fn write_to_host_virt(&self, addr: HostVirtAddr, buf: &[u8]) -> Result<()> {
        unsafe {
            core::ptr::copy_nonoverlapping(buf.as_ptr(), addr as *mut u8, buf.len());
        }
        Ok(())
    }

    pub fn write_to_host(&self, addr: HostPhysAddr, buf: &[u8]) -> Result<()> {
        self.write_to_host_virt(phys_to_virt(addr.into()).as_usize(), buf)
    }

    pub fn write_object_to_host_virt<T: ByteCode>(
        &self,
        addr: HostVirtAddr,
        obj: &T,
    ) -> Result<()> {
        let buf = obj.as_bytes();
        self.write_to_host_virt(addr, buf)
    }

    pub fn write_object_to_host<T: ByteCode>(&self, addr: HostPhysAddr, obj: &T) -> Result<()> {
        let buf = obj.as_bytes();
        self.write_to_host(addr, buf)
    }

    pub fn read_from_guest(&self, addr: GuestPhysAddr, buf: &mut [u8]) -> Result<()> {
        let host_addr = self.translate(addr)?;

        self.read_from_host(host_addr, buf)
    }

    pub fn read_object_from_guest<T: ByteCode>(&self, addr: GuestPhysAddr) -> Result<T> {
        let mut obj = T::default();
        let buf = obj.as_mut_bytes();
        self.read_from_guest(addr, buf)?;
        Ok(obj)
    }

    pub fn write_to_guest(&self, addr: GuestPhysAddr, buf: &[u8]) -> Result<()> {
        let host_addr = self.translate(addr)?;

        self.write_to_host(host_addr, buf)
    }

    pub fn write_object_to_guest<T: ByteCode>(&self, addr: GuestPhysAddr, obj: &T) -> Result<()> {
        let buf = obj.as_bytes();
        self.write_to_guest(addr, buf)
    }

    /// Convert GPA buffer iovec to HVA buffer iovec.
    ///
    /// # Arguments
    ///
    /// * `addr` - Guest address.
    /// * `count` - Memory needed length
    pub fn get_address_map(
        &self,
        addr: GuestPhysAddr,
        count: u64,
        res: &mut Vec<Iovec>,
    ) -> Result<()> {
        let mut len = count;
        let mut start = addr as u64;

        loop {
            let io_vec = self
                .translate_and_get_limit(addr)
                .map(|(hva, region_len)| Iovec {
                    iov_base: hva as u64,
                    iov_len: core::cmp::min(len, region_len as u64),
                })?;

            start += io_vec.iov_len;
            len -= io_vec.iov_len;
            res.push(io_vec);

            if len == 0 {
                break;
            }
        }

        Ok(())
    }

    pub fn get_region_cache(&self, addr: GuestPhysAddr) -> Option<TranslatedRegion> {
        let guest_region = self.inner.read().lookup_region(addr);

        match self.inner.read().lookup_region(addr) {
            Ok(guest_region) => Some(TranslatedRegion {
                hva_base: self.translate(guest_region.hpa).expect("Failed to perform translating in get_region_cache"),
                gpa_start: guest_region.gpa,
                gpa_end: guest_region.gpa + guest_region.size,
            }),
            Err(e) => None,
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub struct TranslatedRegion {
    // pub reg_type: RegionType,
    pub hva_base: HostVirtAddr,
    pub gpa_start: GuestPhysAddr,
    pub gpa_end: GuestPhysAddr,
}

impl TranslatedRegion {
    pub fn out_of_range(&self, addr: GuestPhysAddr) -> bool {
        addr < self.gpa_start && addr >= self.gpa_end
    }
}

#[derive(Debug, PartialEq, Eq, Copy, Clone)]
pub enum RegionType {
    /// Ram type.
    Ram,
    /// IO type.
    IO,
    /// Container type.
    Container,
    /// RomDevice type.
    RomDevice,
    /// RamDevice type.
    RamDevice,
    /// Alias type
    Alias,
}
