// Copyright (c) 2020 Huawei Technologies Co.,Ltd. All rights reserved.
//
// StratoVirt is licensed under Mulan PSL v2.
// You can use this software according to the terms and conditions of the Mulan
// PSL v2.
// You may obtain a copy of Mulan PSL v2 at:
//         http://license.coscl.org.cn/MulanPSL2
// THIS SOFTWARE IS PROVIDED ON AN "AS IS" BASIS, WITHOUT WARRANTIES OF ANY
// KIND, EITHER EXPRESS OR IMPLIED, INCLUDING BUT NOT LIMITED TO
// NON-INFRINGEMENT, MERCHANTABILITY OR FIT FOR A PARTICULAR PURPOSE.
// See the Mulan PSL v2 for more details.

/*
use std::cmp::min;
use std::mem::size_of;
use std::num::Wrapping;
use std::ops::{Deref, DerefMut};
use std::sync::atomic::{fence, AtomicBool, Ordering};
use std::sync::Arc;
*/

//use anyhow::{anyhow, bail, Context, Result};

use alloc::fmt::format;
use alloc::format;
use alloc::sync::{Arc, Weak};
use core::mem::size_of;
use core::num::Wrapping;
use core::cmp::{max, min};
use core::ops::{Deref, DerefMut};
use core::sync::atomic::{fence, Ordering, AtomicBool};

//use core::sync::atomic::{AtomicU16, };



use log::{error, warn};

use super::{
    checked_offset_mem, ElemIovec, Element, VringOps, INVALID_VECTOR_NUM, VIRTQ_DESC_F_INDIRECT,
    VIRTQ_DESC_F_NEXT, VIRTQ_DESC_F_WRITE,
};

///use crate::error::VirtioError;

/*use crate::{
    report_virtio_error, virtio_has_feature, VirtioError, VirtioInterrupt, VIRTIO_F_RING_EVENT_IDX,
};
*/

use hypercraft::{HyperError, HyperResult, MmioOps, PciError, PioOps, RegionOps, VirtioError};

use crate::device::virtio::{
    report_virtio_error, virtio_has_feature, VirtioInterrupt, VIRTIO_F_RING_EVENT_IDX,
};


//use address_space::{AddressSpace, GuestAddress, RegionCache, RegionType};

use pci::util::byte_code::ByteCode;

/// When host consumes a buffer, don't interrupt the guest.
const VRING_AVAIL_F_NO_INTERRUPT: u16 = 1;
/// When guest produces a buffer, don't notify the host.
const VRING_USED_F_NO_NOTIFY: u16 = 1;

/// Max total len of a descriptor chain.
const DESC_CHAIN_MAX_TOTAL_LEN: u64 = 1u64 << 32;
/// The length of used element.
const USEDELEM_LEN: u64 = size_of::<UsedElem>() as u64;
/// The length of avail element.
const AVAILELEM_LEN: u64 = size_of::<u16>() as u64;
/// The length of available ring except array of avail element(flags: u16 idx: u16 used_event: u16).
const VRING_AVAIL_LEN_EXCEPT_AVAILELEM: u64 = (size_of::<u16>() * 3) as u64;
/// The length of used ring except array of used element(flags: u16 idx: u16 avail_event: u16).
const VRING_USED_LEN_EXCEPT_USEDELEM: u64 = (size_of::<u16>() * 3) as u64;
/// The length of flags(u16) and idx(u16).
const VRING_FLAGS_AND_IDX_LEN: u64 = size_of::<SplitVringFlagsIdx>() as u64;
/// The position of idx in the available ring and the used ring.
const VRING_IDX_POSITION: u64 = size_of::<u16>() as u64;
/// The length of virtio descriptor.
const DESCRIPTOR_LEN: u64 = size_of::<SplitVringDesc>() as u64;

#[derive(Default, Clone, Copy)]
pub struct VirtioAddrCache {
    /// Host virtual address of the descriptor table.
    pub desc_table_host: u64,
    /// Host virtual address of the available ring.
    pub avail_ring_host: u64,
    /// Host virtual address of the used ring.
    pub used_ring_host: u64,
}

/// The configuration of virtqueue.
#[derive(Default, Clone, Copy)]
pub struct QueueConfig {
    /// Guest physical address of the descriptor table.
    pub desc_table: u64,
    /// Guest physical address of the available ring.
    pub avail_ring: u64,
    /// Guest physical address of the used ring.
    pub used_ring: u64,
    /// Host address cache.
    pub addr_cache: VirtioAddrCache,
    /// The maximal size of elements offered by the device.
    pub max_size: u16,
    /// The queue size set by the guest.
    pub size: u16,
    /// Virtual queue ready bit.
    pub ready: bool,
    /// Interrupt vector index of the queue for msix
    pub vector: u16,
    /// The next index which can be popped in the available vring.
    next_avail: Wrapping<u16>,
    /// The next index which can be pushed in the used vring.
    next_used: Wrapping<u16>,
    /// The index of last descriptor used which has triggered interrupt.
    last_signal_used: Wrapping<u16>,
    /// The last_signal_used is valid or not.
    signal_used_valid: bool,
}

impl QueueConfig {
    /// Create configuration for a virtqueue.
    ///
    /// # Arguments
    ///
    /// * `max_size` - The maximum size of the virtqueue.
    pub fn new(max_size: u16) -> Self {
        let addr_cache = VirtioAddrCache::default();
        QueueConfig {
            desc_table: 0,
            avail_ring: 0,
            used_ring: 0,
            addr_cache,
            max_size,
            size: max_size,
            ready: false,
            vector: INVALID_VECTOR_NUM,
            next_avail: Wrapping(0),
            next_used: Wrapping(0),
            last_signal_used: Wrapping(0),
            signal_used_valid: false,
        }
    }

    fn get_desc_size(&self) -> u64 {
        min(self.size, self.max_size) as u64 * DESCRIPTOR_LEN
    }

    fn get_used_size(&self, features: u64) -> u64 {
        let size = if virtio_has_feature(features, VIRTIO_F_RING_EVENT_IDX) {
            2_u64
        } else {
            0_u64
        };

        size + VRING_FLAGS_AND_IDX_LEN + (min(self.size, self.max_size) as u64) * USEDELEM_LEN
    }

    fn get_avail_size(&self, features: u64) -> u64 {
        let size = if virtio_has_feature(features, VIRTIO_F_RING_EVENT_IDX) {
            2_u64
        } else {
            0_u64
        };

        size + VRING_FLAGS_AND_IDX_LEN
            + (min(self.size, self.max_size) as u64) * (size_of::<u16>() as u64)
    }

    pub fn reset(&mut self) {
        *self = Self::new(self.max_size);
    }

    pub fn set_addr_cache(
        &mut self,
        mem_space: Arc<AddressSpace>,
        interrupt_cb: Arc<VirtioInterrupt>,
        features: u64,
        broken: &Arc<AtomicBool>,
    ) {
        self.addr_cache.desc_table_host =
            if let Some((addr, size)) = mem_space.addr_cache_init(self.desc_table) {
                if size < self.get_desc_size() {
                    report_virtio_error(interrupt_cb.clone(), features, broken);
                    0_u64
                } else {
                    addr
                }
            } else {
                0_u64
            };

        self.addr_cache.avail_ring_host =
            if let Some((addr, size)) = mem_space.addr_cache_init(self.avail_ring) {
                if size < self.get_avail_size(features) {
                    report_virtio_error(interrupt_cb.clone(), features, broken);
                    0_u64
                } else {
                    addr
                }
            } else {
                0_u64
            };

        self.addr_cache.used_ring_host =
            if let Some((addr, size)) = mem_space.addr_cache_init(self.used_ring) {
                if size < self.get_used_size(features) {
                    report_virtio_error(interrupt_cb.clone(), features, broken);
                    0_u64
                } else {
                    addr
                }
            } else {
                0_u64
            };
    }
}

/// Virtio used element.
#[repr(C)]
#[derive(Default, Clone, Copy)]
struct UsedElem {
    /// Index of descriptor in the virqueue descriptor table.
    id: u32,
    /// Total length of the descriptor chain which was used (written to).
    len: u32,
}

impl ByteCode for UsedElem {}

/// A struct including flags and idx for avail vring and used vring.
#[repr(C)]
#[derive(Default, Clone, Copy)]
struct SplitVringFlagsIdx {
    flags: u16,
    idx: u16,
}

impl ByteCode for SplitVringFlagsIdx {}

struct DescInfo {
    /// The host virtual address of the descriptor table.
    table_host: u64,
    /// The size of the descriptor table.
    size: u16,
    /// The index of the current descriptor table.
    index: u16,
    /// The descriptor table.
    desc: SplitVringDesc,
}

/// Descriptor of split vring.
#[repr(C)]
#[derive(Default, Clone, Copy)]
pub struct SplitVringDesc {
    /// Address (guest-physical).
    pub addr: u64,
    /// Length.
    pub len: u32,
    /// The flags as indicated above.
    pub flags: u16,
    /// We chain unused descriptors via this, too.
    pub next: u16,
}

impl SplitVringDesc {
    /// Create a descriptor of split vring.
    ///
    /// # Arguments
    ///
    /// * `sys_mem` - Address space to which the vring belongs.
    /// * `desc_table` - Guest address of virtqueue descriptor table.
    /// * `queue_size` - Size of virtqueue.
    /// * `index` - Index of descriptor in the virqueue descriptor table.
    fn new(
        sys_mem: &Arc<AddressSpace>,
        desc_table_host: u64,
        queue_size: u16,
        index: u16,
        cache: &mut Option<RegionCache>,
    ) -> HyperResult<Self> {
        if index >= queue_size {
            return Err(HyperError::PciError(VirtioError::QueueIndex(index, queue_size)));
        }

        let desc_addr = desc_table_host
            .checked_add(u64::from(index) * DESCRIPTOR_LEN)
            .with_context(|| {
                VirtioError::AddressOverflow(
                    "creating a descriptor",
                    desc_table_host,
                    u64::from(index) * DESCRIPTOR_LEN,
                )
            })?;
        let desc = sys_mem
            .read_object_direct::<SplitVringDesc>(desc_addr)
            .with_context(|| VirtioError::ReadObjectErr("a descriptor", desc_addr))?;

        if desc.is_valid(sys_mem, queue_size, cache) {
            Ok(desc)
        } else {
            Err(HyperError::PciError(VirtioError::QueueDescInvalid))
        }
    }

    /// Return true if the descriptor is valid.
    fn is_valid(
        &self,
        sys_mem: &Arc<AddressSpace>,
        queue_size: u16,
        cache: &mut Option<RegionCache>,
    ) -> bool {
        if self.len == 0 {
            error!("Zero sized buffers are not allowed");
            return false;
        }
        let mut miss_cached = true;
        if let Some(reg_cache) = cache {
            let base = self.addr.0;
            let offset = self.len as u64;
            let end = match base.checked_add(offset) {
                Some(addr) => addr,
                None => {
                    error!("The memory of descriptor is invalid, range overflows");
                    return false;
                }
            };
            if base > reg_cache.start && end < reg_cache.end {
                miss_cached = false;
            }
        } else {
            let gotten_cache = sys_mem.get_region_cache(self.addr);
            if let Some(obtained_cache) = gotten_cache {
                if obtained_cache.reg_type == RegionType::Ram {
                    *cache = gotten_cache;
                }
            }
        }

        if miss_cached {
            if let Err(ref e) = checked_offset_mem(sys_mem, self.addr, u64::from(self.len)) {
                error!("The memory of descriptor is invalid, {:?} ", e);
                return false;
            }
        }

        if self.has_next() && self.next >= queue_size {
            error!(
                "The next index {} exceed queue size {}",
                self.next, queue_size,
            );
            return false;
        }

        true
    }

    /// Return true if this descriptor has next descriptor.
    fn has_next(&self) -> bool {
        self.flags & VIRTQ_DESC_F_NEXT != 0
    }

    /// Get the next descriptor in descriptor chain.
    fn next_desc(
        sys_mem: &Arc<AddressSpace>,
        desc_table_host: u64,
        queue_size: u16,
        index: u16,
        cache: &mut Option<RegionCache>,
    ) -> HyperResult<SplitVringDesc> {
        SplitVringDesc::new(sys_mem, desc_table_host, queue_size, index, cache)
            .with_context(|| format!("Failed to find next descriptor {}", index))
    }

    /// Check whether this descriptor is write-only or read-only.
    /// Write-only means that the emulated device can write and the driver can read.
    fn write_only(&self) -> bool {
        self.flags & VIRTQ_DESC_F_WRITE != 0
    }

    /// Return true if this descriptor is a indirect descriptor.
    fn is_indirect_desc(&self) -> bool {
        self.flags & VIRTQ_DESC_F_INDIRECT != 0
    }

    /// Return true if the indirect descriptor is valid.
    /// The len can be divided evenly by the size of descriptor and can not be zero.
    fn is_valid_indirect_desc(&self) -> bool {
        if self.len == 0
            || u64::from(self.len) % DESCRIPTOR_LEN != 0
            || u64::from(self.len) / DESCRIPTOR_LEN > u16::MAX as u64
        {
            error!("The indirect descriptor is invalid, len: {}", self.len);
            return false;
        }
        if self.has_next() {
            error!("INDIRECT and NEXT flag should not be used together");
            return false;
        }
        true
    }

    /// Get the num of descriptor in the table of indirect descriptor.
    fn get_desc_num(&self) -> u16 {
        (u64::from(self.len) / DESCRIPTOR_LEN) as u16
    }

    /// Get element from descriptor chain.
    fn get_element(
        sys_mem: &Arc<AddressSpace>,
        desc_info: &DescInfo,
        cache: &mut Option<RegionCache>,
        elem: &mut Element,
    ) -> HyperResult<()> {
        let mut desc_table_host = desc_info.table_host;
        let mut desc_size = desc_info.size;
        let mut desc = desc_info.desc;
        elem.index = desc_info.index;
        let mut queue_size = desc_size;
        let mut indirect: bool = false;
        let mut write_elem_count: u32 = 0;
        let mut desc_total_len: u64 = 0;

        loop {
            if elem.desc_num >= desc_size {
                error!("The element desc number exceeds max allowed");
            }

            if desc.is_indirect_desc() {
                if !desc.is_valid_indirect_desc() {
                    return Err(HyperError::PciError(VirtioError::QueueDescInvalid));
                }
                if !indirect {
                    indirect = true;
                } else {
                    error!("Found two indirect descriptor elem in one request");
                }
                (desc_table_host, _) = sys_mem
                    .get_host_address_from_cache(desc.addr, cache)
                    .with_context(|| "Failed to get descriptor table entry host address")?;
                queue_size = desc.get_desc_num();
                desc = Self::next_desc(sys_mem, desc_table_host, queue_size, 0, cache)?;
                desc_size = elem
                    .desc_num
                    .checked_add(queue_size)
                    .with_context(|| "The chained desc number overflows")?;
                continue;
            }

            let iovec = ElemIovec {
                addr: desc.addr,
                len: desc.len,
            };

            if desc.write_only() {
                elem.in_iovec.push(iovec);
                write_elem_count += 1;
            } else {
                if write_elem_count > 0 {
                    error!("Invalid order of the descriptor elem");
                }
                elem.out_iovec.push(iovec);
            }
            elem.desc_num += 1;
            desc_total_len += iovec.len as u64;

            if desc.has_next() {
                desc = Self::next_desc(sys_mem, desc_table_host, queue_size, desc.next, cache)?;
            } else {
                break;
            }
        }

        if desc_total_len > DESC_CHAIN_MAX_TOTAL_LEN {
            error!("Find a descriptor chain longer than 4GB in total");
        }

        Ok(())
    }
}

impl ByteCode for SplitVringDesc {}

/// Split vring.
#[derive(Default, Clone, Copy)]
pub struct SplitVring {
    /// Region cache information.
    cache: Option<RegionCache>,
    /// The configuration of virtqueue.
    queue_config: QueueConfig,
}

impl Deref for SplitVring {
    type Target = QueueConfig;
    fn deref(&self) -> &Self::Target {
        &self.queue_config
    }
}

impl DerefMut for SplitVring {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.queue_config
    }
}

impl SplitVring {
    /// Create a split vring.
    ///
    /// # Arguments
    ///
    /// * `queue_config` - Configuration of the vring.
    pub fn new(queue_config: QueueConfig) -> Self {
        SplitVring {
            cache: None,
            queue_config,
        }
    }

    /// The actual size of the queue.
    fn actual_size(&self) -> u16 {
        min(self.size, self.max_size)
    }

    /// Get the flags and idx of the available ring from guest memory.
    fn get_avail_flags_idx(&self, sys_mem: &Arc<AddressSpace>) -> HyperResult<SplitVringFlagsIdx> {
        sys_mem
            .read_object_direct::<SplitVringFlagsIdx>(self.addr_cache.avail_ring_host)
            .with_context(|| {
                VirtioError::ReadObjectErr("avail flags idx", self.avail_ring.raw_value())
            })
    }

    /// Get the idx of the available ring from guest memory.
    fn get_avail_idx(&self, sys_mem: &Arc<AddressSpace>) -> HyperResult<u16> {
        let flags_idx = self.get_avail_flags_idx(sys_mem)?;
        Ok(flags_idx.idx)
    }

    /// Get the flags of the available ring from guest memory.
    fn get_avail_flags(&self, sys_mem: &Arc<AddressSpace>) -> HyperResult<u16> {
        let flags_idx = self.get_avail_flags_idx(sys_mem)?;
        Ok(flags_idx.flags)
    }

    /// Get the flags and idx of the used ring from guest memory.
    fn get_used_flags_idx(&self, sys_mem: &Arc<AddressSpace>) -> HyperResult<SplitVringFlagsIdx> {
        // Make sure the idx read from sys_mem is new.
        fence(Ordering::SeqCst);
        sys_mem
            .read_object_direct::<SplitVringFlagsIdx>(self.addr_cache.used_ring_host)
            .with_context(|| {
                VirtioError::ReadObjectErr("used flags idx", self.used_ring.raw_value())
            })
    }

    /// Get the index of the used ring from guest memory.
    fn get_used_idx(&self, sys_mem: &Arc<AddressSpace>) -> HyperResult<u16> {
        let flag_idx = self.get_used_flags_idx(sys_mem)?;
        Ok(flag_idx.idx)
    }

    /// Set the used flags to suppress virtqueue notification or not
    fn set_used_flags(&self, sys_mem: &Arc<AddressSpace>, suppress: bool) -> HyperResult<()> {
        let mut flags_idx = self.get_used_flags_idx(sys_mem)?;

        if suppress {
            flags_idx.flags |= VRING_USED_F_NO_NOTIFY;
        } else {
            flags_idx.flags &= !VRING_USED_F_NO_NOTIFY;
        }
        sys_mem
            .write_object_direct::<SplitVringFlagsIdx>(&flags_idx, self.addr_cache.used_ring_host)
            .with_context(|| {
                format!(
                    "Failed to set used flags, used_ring: 0x{:X}",
                    self.used_ring.raw_value()
                )
            })?;
        // Make sure the data has been set.
        fence(Ordering::SeqCst);
        Ok(())
    }

    /// Set the avail idx to the field of the event index for the available ring.
    fn set_avail_event(&self, sys_mem: &Arc<AddressSpace>, event_idx: u16) -> HyperResult<()> {
        //trace::virtqueue_set_avail_event(self as *const _ as u64, event_idx);
        let avail_event_offset =
            VRING_FLAGS_AND_IDX_LEN + USEDELEM_LEN * u64::from(self.actual_size());

        sys_mem
            .write_object_direct(
                &event_idx,
                self.addr_cache.used_ring_host + avail_event_offset,
            )
            .with_context(|| {
                format!(
                    "Failed to set avail event idx, used_ring: 0x{:X}, offset: {}",
                    self.used_ring.raw_value(),
                    avail_event_offset,
                )
            })?;
        // Make sure the data has been set.
        fence(Ordering::SeqCst);
        Ok(())
    }

    /// Get the event index of the used ring from guest memory.
    fn get_used_event(&self, sys_mem: &Arc<AddressSpace>) -> HyperResult<u16> {
        let used_event_offset =
            VRING_FLAGS_AND_IDX_LEN + AVAILELEM_LEN * u64::from(self.actual_size());
        // Make sure the event idx read from sys_mem is new.
        fence(Ordering::SeqCst);
        // The GPA of avail_ring_host with avail table length has been checked in
        // is_invalid_memory which must not be overflowed.
        let used_event_addr = self.addr_cache.avail_ring_host + used_event_offset;
        let used_event = sys_mem
            .read_object_direct::<u16>(used_event_addr)
            .with_context(|| VirtioError::ReadObjectErr("used event id", used_event_addr))?;

        Ok(used_event)
    }

    /// Return true if VRING_AVAIL_F_NO_INTERRUPT is set.
    fn is_avail_ring_no_interrupt(&self, sys_mem: &Arc<AddressSpace>) -> bool {
        match self.get_avail_flags(sys_mem) {
            Ok(avail_flags) => (avail_flags & VRING_AVAIL_F_NO_INTERRUPT) != 0,
            Err(ref e) => {
                warn!(
                    "Failed to get the status for VRING_AVAIL_F_NO_INTERRUPT {:?}",
                    e
                );
                false
            }
        }
    }

    /// Return true if it's required to trigger interrupt for the used vring.
    fn used_ring_need_event(&mut self, sys_mem: &Arc<AddressSpace>) -> bool {
        let old = self.last_signal_used;
        let new = match self.get_used_idx(sys_mem) {
            Ok(used_idx) => Wrapping(used_idx),
            Err(ref e) => {
                error!("Failed to get the status for notifying used vring: {:?}", e);
                return false;
            }
        };

        let used_event_idx = match self.get_used_event(sys_mem) {
            Ok(idx) => Wrapping(idx),
            Err(ref e) => {
                error!("Failed to get the status for notifying used vring: {:?}", e);
                return false;
            }
        };

        let valid = self.signal_used_valid;
        self.signal_used_valid = true;
        self.last_signal_used = new;
        !valid || (new - used_event_idx - Wrapping(1)) < (new - old)
    }

    fn is_overlap(
        start1: u64,
        end1: u64,
        start2: u64,
        end2: u64,
    ) -> bool {
        !(start1 >= end2 || start2 >= end1)
    }

    fn is_invalid_memory(&self, sys_mem: &Arc<AddressSpace>, actual_size: u64) -> bool {
        let desc_table_end =
            match checked_offset_mem(sys_mem, self.desc_table, DESCRIPTOR_LEN * actual_size) {
                Ok(addr) => addr,
                Err(ref e) => {
                    error!(
                        "descriptor table is out of bounds: start:0x{:X} size:{} {:?}",
                        self.desc_table.raw_value(),
                        DESCRIPTOR_LEN * actual_size,
                        e
                    );
                    return true;
                }
            };

        let desc_avail_end = match checked_offset_mem(
            sys_mem,
            self.avail_ring,
            VRING_AVAIL_LEN_EXCEPT_AVAILELEM + AVAILELEM_LEN * actual_size,
        ) {
            Ok(addr) => addr,
            Err(ref e) => {
                error!(
                    "avail ring is out of bounds: start:0x{:X} size:{} {:?}",
                    self.avail_ring.raw_value(),
                    VRING_AVAIL_LEN_EXCEPT_AVAILELEM + AVAILELEM_LEN * actual_size,
                    e
                );
                return true;
            }
        };

        let desc_used_end = match checked_offset_mem(
            sys_mem,
            self.used_ring,
            VRING_USED_LEN_EXCEPT_USEDELEM + USEDELEM_LEN * actual_size,
        ) {
            Ok(addr) => addr,
            Err(ref e) => {
                error!(
                    "used ring is out of bounds: start:0x{:X} size:{} {:?}",
                    self.used_ring.raw_value(),
                    VRING_USED_LEN_EXCEPT_USEDELEM + USEDELEM_LEN * actual_size,
                    e,
                );
                return true;
            }
        };

        if SplitVring::is_overlap(
            self.desc_table,
            desc_table_end,
            self.avail_ring,
            desc_avail_end,
        ) || SplitVring::is_overlap(
            self.avail_ring,
            desc_avail_end,
            self.used_ring,
            desc_used_end,
        ) || SplitVring::is_overlap(
            self.desc_table,
            desc_table_end,
            self.used_ring,
            desc_used_end,
        ) {
            error!("The memory of descriptor table: 0x{:X}, avail ring: 0x{:X} or used ring: 0x{:X} is overlapped. queue size:{}",
                   self.desc_table.raw_value(), self.avail_ring.raw_value(), self.used_ring.raw_value(), actual_size);
            return true;
        }

        if self.desc_table.0 & 0xf != 0 {
            error!(
                "descriptor table: 0x{:X} is not aligned",
                self.desc_table.raw_value()
            );
            true
        } else if self.avail_ring.0 & 0x1 != 0 {
            error!(
                "avail ring: 0x{:X} is not aligned",
                self.avail_ring.raw_value()
            );
            true
        } else if self.used_ring.0 & 0x3 != 0 {
            error!(
                "used ring: 0x{:X} is not aligned",
                self.used_ring.raw_value()
            );
            true
        } else {
            false
        }
    }

    fn get_desc_info(
        &mut self,
        sys_mem: &Arc<AddressSpace>,
        next_avail: Wrapping<u16>,
        features: u64,
    ) -> HyperResult<DescInfo> {
        let index_offset =
            VRING_FLAGS_AND_IDX_LEN + AVAILELEM_LEN * u64::from(next_avail.0 % self.actual_size());
        // The GPA of avail_ring_host with avail table length has been checked in
        // is_invalid_memory which must not be overflowed.
        let desc_index_addr = self.addr_cache.avail_ring_host + index_offset;
        let desc_index = sys_mem
            .read_object_direct::<u16>(desc_index_addr)
            .with_context(|| {
                VirtioError::ReadObjectErr("the index of descriptor", desc_index_addr)
            })?;

        let desc = SplitVringDesc::new(
            sys_mem,
            self.addr_cache.desc_table_host,
            self.actual_size(),
            desc_index,
            &mut self.cache,
        )?;

        // Suppress queue notification related to current processing desc chain.
        if virtio_has_feature(features, VIRTIO_F_RING_EVENT_IDX) {
            self.set_avail_event(sys_mem, (next_avail + Wrapping(1)).0)
                .with_context(|| "Failed to set avail event for popping avail ring")?;
        }

        Ok(DescInfo {
            table_host: self.addr_cache.desc_table_host,
            size: self.actual_size(),
            index: desc_index,
            desc,
        })
    }

    fn get_vring_element(
        &mut self,
        sys_mem: &Arc<AddressSpace>,
        features: u64,
        elem: &mut Element,
    ) -> HyperResult<()> {
        let desc_info = self.get_desc_info(sys_mem, self.next_avail, features)?;

        SplitVringDesc::get_element(sys_mem, &desc_info, &mut self.cache, elem).with_context(
            || {
                format!(
                    "Failed to get element from descriptor chain {}, table addr: 0x{:X}, size: {}",
                    desc_info.index, desc_info.table_host, desc_info.size,
                )
            },
        )?;
        self.next_avail += Wrapping(1);

        Ok(())
    }
}

impl VringOps for SplitVring {
    fn is_enabled(&self) -> bool {
        self.ready
    }

    fn is_valid(&self, sys_mem: &Arc<AddressSpace>) -> bool {
        let size = u64::from(self.actual_size());
        if !self.ready {
            error!("The configuration of vring is not ready\n");
            false
        } else if self.size > self.max_size || self.size == 0 || (self.size & (self.size - 1)) != 0
        {
            error!(
                "vring with invalid size:{} max size:{}",
                self.size, self.max_size
            );
            false
        } else {
            !self.is_invalid_memory(sys_mem, size)
        }
    }

    fn pop_avail(&mut self, sys_mem: &Arc<AddressSpace>, features: u64) -> HyperResult<Element> {
        let mut element = Element::new(0);
        if !self.is_enabled() || self.avail_ring_len(sys_mem)? == 0 {
            return Ok(element);
        }

        // Make sure descriptor read does not bypass avail index read.
        fence(Ordering::Acquire);

        self.get_vring_element(sys_mem, features, &mut element)
            .with_context(|| "Failed to get vring element")?;

        /*
            trace::virtqueue_pop_avail(
            &*self as *const _ as u64,
            element.in_iovec.len(),
            element.out_iovec.len(),
        );
        */

        Ok(element)
    }

    fn push_back(&mut self) {
        self.next_avail -= Wrapping(1);
    }

    fn add_used(&mut self, sys_mem: &Arc<AddressSpace>, index: u16, len: u32) -> HyperResult<()> {
        if index >= self.size {
            return Err(HyperError::PciError(VirtioError::QueueIndex(index, self.size)));
        }

        let next_used = u64::from(self.next_used.0 % self.actual_size());
        //trace::virtqueue_add_used(&*self as *const _ as u64, next_used, index, len);
        let used_elem_addr =
            self.addr_cache.used_ring_host + VRING_FLAGS_AND_IDX_LEN + next_used * USEDELEM_LEN;
        let used_elem = UsedElem {
            id: u32::from(index),
            len,
        };
        sys_mem
            .write_object_direct::<UsedElem>(&used_elem, used_elem_addr)
            .with_context(|| "Failed to write object for used element")?;
        // Make sure used element is filled before updating used idx.
        fence(Ordering::Release);

        self.next_used += Wrapping(1);
        sys_mem
            .write_object_direct(
                &(self.next_used.0),
                self.addr_cache.used_ring_host + VRING_IDX_POSITION,
            )
            .with_context(|| "Failed to write next used idx")?;
        // Make sure used index is exposed before notifying guest.
        fence(Ordering::SeqCst);

        // Do we wrap around?
        if self.next_used == self.last_signal_used {
            self.signal_used_valid = false;
        }
        Ok(())
    }

    fn should_notify(&mut self, sys_mem: &Arc<AddressSpace>, features: u64) -> bool {
        if virtio_has_feature(features, VIRTIO_F_RING_EVENT_IDX) {
            self.used_ring_need_event(sys_mem)
        } else {
            !self.is_avail_ring_no_interrupt(sys_mem)
        }
    }

    fn suppress_queue_notify(
        &mut self,
        sys_mem: &Arc<AddressSpace>,
        features: u64,
        suppress: bool,
    ) -> HyperResult<()> {
        if virtio_has_feature(features, VIRTIO_F_RING_EVENT_IDX) {
            self.set_avail_event(sys_mem, self.get_avail_idx(sys_mem)?)?;
        } else {
            self.set_used_flags(sys_mem, suppress)?;
        }
        Ok(())
    }

    fn actual_size(&self) -> u16 {
        self.actual_size()
    }

    fn get_queue_config(&self) -> QueueConfig {
        let mut config = self.queue_config;
        config.signal_used_valid = false;
        config
    }

    /// The number of descriptor chains in the available ring.
    fn avail_ring_len(&mut self, sys_mem: &Arc<AddressSpace>) -> HyperResult<u16> {
        let avail_idx = self.get_avail_idx(sys_mem).map(Wrapping)?;

        Ok((avail_idx - self.next_avail).0)
    }

    fn get_avail_idx(&self, sys_mem: &Arc<AddressSpace>) -> HyperResult<u16> {
        SplitVring::get_avail_idx(self, sys_mem)
    }

    fn get_used_idx(&self, sys_mem: &Arc<AddressSpace>) -> HyperResult<u16> {
        SplitVring::get_used_idx(self, sys_mem)
    }

    fn get_cache(&self) -> &Option<RegionCache> {
        &self.cache
    }

    fn get_avail_bytes(
        &mut self,
        sys_mem: &Arc<AddressSpace>,
        max_size: usize,
        is_in: bool,
    ) -> HyperResult<usize> {
        if !self.is_enabled() {
            return Ok(0);
        }
        fence(Ordering::Acquire);

        let mut avail_bytes = 0_usize;
        let mut avail_idx = self.next_avail;
        let end_idx = self.get_avail_idx(sys_mem).map(Wrapping)?;
        while (end_idx - avail_idx).0 > 0 {
            let desc_info = self.get_desc_info(sys_mem, avail_idx, 0)?;

            let mut elem = Element::new(0);
            SplitVringDesc::get_element(sys_mem, &desc_info, &mut self.cache, &mut elem).with_context(
                || {
                    format!(
                        "Failed to get element from descriptor chain {}, table addr: 0x{:X}, size: {}",
                        desc_info.index, desc_info.table_host, desc_info.size,
                    )
                },
            )?;

            for e in match is_in {
                true => elem.in_iovec,
                false => elem.out_iovec,
            } {
                avail_bytes += e.len as usize;
            }

            if avail_bytes >= max_size {
                return Ok(max_size);
            }
            avail_idx += Wrapping(1);
        }
        Ok(avail_bytes)
    }
}
