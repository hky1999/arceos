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

use alloc::rc::Rc;
use alloc::sync::Arc;
use bitflags::Flags;
use core::cmp;
use core::mem::size_of;
use core::sync::atomic::{AtomicBool, Ordering};

use alloc::boxed::Box;
use alloc::format;
use alloc::string::{String, ToString};
use alloc::vec;
use alloc::vec::Vec;

// use anyhow::{anyhow, bail, Context, Result};
use byteorder::{ByteOrder, LittleEndian};
use spin::Mutex;

use hypercraft::{HyperError, HyperResult as Result};

use crate::device::virtio::features::BlkFeature;
use crate::device::virtio::{
    check_config_space_rw, gpa_hva_iovec_map, iov_discard_back, iov_discard_front, iov_to_buf,
    read_config_default, report_virtio_error, virtio_has_feature, Element, Queue, VirtioBase,
    VirtioDevice, VirtioError, VirtioInterrupt, VirtioInterruptType, VIRTIO_BLK_ID_BYTES,
    VIRTIO_BLK_S_IOERR, VIRTIO_BLK_S_OK, VIRTIO_BLK_S_UNSUPP, VIRTIO_BLK_T_DISCARD,
    VIRTIO_BLK_T_FLUSH, VIRTIO_BLK_T_GET_ID, VIRTIO_BLK_T_IN, VIRTIO_BLK_T_OUT,
    VIRTIO_BLK_T_WRITE_ZEROES, VIRTIO_BLK_WRITE_ZEROES_FLAG_UNMAP, VIRTIO_F_RING_EVENT_IDX,
    VIRTIO_F_RING_INDIRECT_DESC, VIRTIO_F_VERSION_1, VIRTIO_TYPE_BLOCK,
};
use crate::mm::iovec::{iov_from_buf_direct, iov_to_buf_direct, Iovec};
use crate::mm::AddressSpace;
use crate::pci::util::byte_code::ByteCode;
use crate::GuestPhysAddr;
use pci::offset_of;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WriteZeroesState {
    Off,
    On,
    Unmap,
}

/// Number of virtqueues.
const QUEUE_NUM_BLK: usize = 1;
/// Used to compute the number of sectors.
const SECTOR_SHIFT: u8 = 9;
/// Size of a sector of the block device.
const SECTOR_SIZE: u64 = (0x01_u64) << SECTOR_SHIFT;
/// Size of the dummy block device.
const DUMMY_IMG_SIZE: u64 = 0;
/// Max number reqs of a merged request.
const MAX_NUM_MERGE_REQS: u16 = 32;
/// Max number iovs of a merged request.
const MAX_NUM_MERGE_IOVS: usize = 1024;
/// Max number bytes of a merged request.
const MAX_NUM_MERGE_BYTES: u64 = i32::MAX as u64;
/// Max time for every round of process queue.
const MAX_MILLIS_TIME_PROCESS_QUEUE: u16 = 100;
/// Max number sectors of per request.
const MAX_REQUEST_SECTORS: u32 = u32::MAX >> SECTOR_SHIFT;

fn get_serial_num_config(serial_num: &str) -> Vec<u8> {
    let mut id_bytes = vec![0; VIRTIO_BLK_ID_BYTES as usize];
    let bytes_to_copy = cmp::min(serial_num.len(), VIRTIO_BLK_ID_BYTES as usize);

    let serial_bytes = serial_num.as_bytes();
    id_bytes[..bytes_to_copy].clone_from_slice(&serial_bytes[..bytes_to_copy]);
    id_bytes
}

#[repr(C)]
#[derive(Default, Clone, Copy)]
struct RequestOutHeader {
    request_type: u32,
    io_prio: u32,
    sector: u64,
}

impl ByteCode for RequestOutHeader {}

/// The request of discard and write-zeroes use the same struct.
#[repr(C)]
#[derive(Default, Clone, Copy)]
struct DiscardWriteZeroesSeg {
    /// The start sector for discard or write-zeroes.
    sector: u64,
    /// The number of sectors for discard or write-zeroes.
    num_sectors: u32,
    /// The flags used for this range.
    flags: u32,
}

impl ByteCode for DiscardWriteZeroesSeg {}

#[derive(Clone)]
struct Request {
    desc_index: u16,
    out_header: RequestOutHeader,
    iovec: Vec<Iovec>,
    data_len: u64,
    in_len: u32,
    in_header: GuestPhysAddr,
    /// Point to the next merged Request.
    next: Box<Option<Request>>,
}

impl Request {
    fn new(handler: &BlockIoHandler, elem: &mut Element, status: &mut u8) -> Result<Self> {
        if elem.out_iovec.is_empty() || elem.in_iovec.is_empty() {
            error!(
                "Missed header for block request: out {} in {} desc num {}",
                elem.out_iovec.len(),
                elem.in_iovec.len(),
                elem.desc_num
            );
            return Err(HyperError::InvalidParam);
        }

        let mut out_header = RequestOutHeader::default();
        iov_to_buf(
            &handler.mem_space,
            &elem.out_iovec,
            out_header.as_mut_bytes(),
        )
        .and_then(|size| {
            if size < size_of::<RequestOutHeader>() {
                error!("Invalid out header for block request: length {}", size);
                return Err(HyperError::BadState);
            }
            Ok(())
        })?;
        out_header.request_type = LittleEndian::read_u32(out_header.request_type.as_bytes());
        out_header.sector = LittleEndian::read_u64(out_header.sector.as_bytes());

        let in_iov_elem = elem.in_iovec.last().unwrap();
        if in_iov_elem.len < 1 {
            error!(
                "Invalid in header for block request: length {}",
                in_iov_elem.len
            );
            return Err(HyperError::BadState);
        }
        // Note: addr plus len has been checked not overflow in virtqueue.
        // let in_header = GuestAddress(in_iov_elem.addr.0 + in_iov_elem.len as u64 - 1);
        let in_header = in_iov_elem.addr + in_iov_elem.len as usize - 1;

        let mut request = Request {
            desc_index: elem.index,
            out_header,
            iovec: Vec::with_capacity(elem.desc_num as usize),
            data_len: 0,
            in_len: 0,
            in_header,
            next: Box::new(None),
        };

        // Count in_len before discard iovec.
        // We always write the last status byte, so count all in_iovs.
        // Note: in_iov and out_iov total len is no more than 1<<32, and
        // out_iov is more than 1, so in_len will not overflow.
        for in_iov in elem.in_iovec.iter() {
            request.in_len += in_iov.len;
        }

        match out_header.request_type {
            VIRTIO_BLK_T_IN
            | VIRTIO_BLK_T_GET_ID
            | VIRTIO_BLK_T_OUT
            | VIRTIO_BLK_T_DISCARD
            | VIRTIO_BLK_T_WRITE_ZEROES => {
                let data_iovec = match out_header.request_type {
                    VIRTIO_BLK_T_OUT | VIRTIO_BLK_T_DISCARD | VIRTIO_BLK_T_WRITE_ZEROES => {
                        iov_discard_front(&mut elem.out_iovec, size_of::<RequestOutHeader>() as u64)
                    }
                    // Otherwise discard the last "status" byte.
                    _ => iov_discard_back(&mut elem.in_iovec, 1),
                }.expect("Empty data for block request");
                // .with_context(|| "Empty data for block request")?;

                let (data_len, iovec) = gpa_hva_iovec_map(data_iovec, &handler.mem_space)?;
                request.data_len = data_len;
                request.iovec = iovec;
            }
            VIRTIO_BLK_T_FLUSH => (),
            others => {
                error!("Request type {} is not supported for block", others);
                *status = VIRTIO_BLK_S_UNSUPP;
            }
        }

        if !request.io_range_valid(handler.disk_sectors) {
            *status = VIRTIO_BLK_S_IOERR;
        }

        Ok(request)
    }

    // fn execute(
    //     &self,
    //     iohandler: &mut BlockIoHandler,
    //     block_backend: Arc<Mutex<dyn BlockDriverOps<AioCompleteCb>>>,
    //     aiocompletecb: AioCompleteCb,
    // ) -> Result<()> {
    //     let mut req = Some(self);
    //     let mut iovecs = Vec::new();
    //     while let Some(req_raw) = req {
    //         for iov in req_raw.iovec.iter() {
    //             let iovec = Iovec {
    //                 iov_base: iov.iov_base,
    //                 iov_len: iov.iov_len,
    //             };
    //             iovecs.push(iovec);
    //         }
    //         req = req_raw.next.as_ref().as_ref();
    //     }
    //     let offset = (aiocompletecb.req.out_header.sector << SECTOR_SHIFT) as usize;
    //     let request_type = self.out_header.request_type;
    //     if MigrationManager::is_active()
    //         && (request_type == VIRTIO_BLK_T_IN || request_type == VIRTIO_BLK_T_GET_ID)
    //     {
    //         // FIXME: mark dirty page needs to be managed by `AddressSpace` crate.
    //         for iov in iovecs.iter() {
    //             // Mark vmm dirty page manually if live migration is active.
    //             MigrationManager::mark_dirty_log(iov.iov_base, iov.iov_len);
    //         }
    //     }
    //     // trace::virtio_blk_execute(request_type, iovecs.len(), offset);

    //     let serial_num = &iohandler.serial_num;
    //     let mut locked_backend = block_backend.lock().unwrap();
    //     match request_type {
    //         VIRTIO_BLK_T_IN => {
    //             locked_backend
    //                 .read_vectored(iovecs, offset, aiocompletecb)
    //                 .with_context(|| "Failed to process block request for reading")?;
    //         }
    //         VIRTIO_BLK_T_OUT => {
    //             locked_backend
    //                 .write_vectored(iovecs, offset, aiocompletecb)
    //                 .with_context(|| "Failed to process block request for writing")?;
    //         }
    //         VIRTIO_BLK_T_FLUSH => {
    //             locked_backend
    //                 .datasync(aiocompletecb)
    //                 .with_context(|| "Failed to process block request for flushing")?;
    //         }
    //         VIRTIO_BLK_T_GET_ID => {
    //             let serial = serial_num.clone().unwrap_or_else(|| String::from(""));
    //             let serial_vec = get_serial_num_config(&serial);
    //             let status = iov_from_buf_direct(&self.iovec, &serial_vec).map_or_else(
    //                 |e| {
    //                     error!("Failed to process block request for getting id, {:?}", e);
    //                     VIRTIO_BLK_S_IOERR
    //                 },
    //                 |_| VIRTIO_BLK_S_OK,
    //             );
    //             aiocompletecb.complete_request(status)?;
    //         }
    //         VIRTIO_BLK_T_DISCARD => {
    //             if !iohandler.discard {
    //                 error!("Device does not support discard");
    //                 return aiocompletecb.complete_request(VIRTIO_BLK_S_UNSUPP);
    //             }
    //             drop(locked_backend);
    //             self.handle_discard_write_zeroes_req(iohandler, aiocompletecb, OpCode::Discard)?;
    //         }
    //         VIRTIO_BLK_T_WRITE_ZEROES => {
    //             if iohandler.write_zeroes == WriteZeroesState::Off {
    //                 error!("Device does not support write-zeroes");
    //                 return aiocompletecb.complete_request(VIRTIO_BLK_S_UNSUPP);
    //             }
    //             drop(locked_backend);
    //             self.handle_discard_write_zeroes_req(
    //                 iohandler,
    //                 aiocompletecb,
    //                 OpCode::WriteZeroes,
    //             )?;
    //         }
    //         // The illegal request type has been handled in method new().
    //         _ => {}
    //     };
    //     Ok(())
    // }

    // fn handle_discard_write_zeroes_req(
    //     &self,
    //     iohandler: &mut BlockIoHandler,
    //     iocompletecb: AioCompleteCb,
    //     opcode: OpCode,
    // ) -> Result<()> {
    //     let size = size_of::<DiscardWriteZeroesSeg>() as u64;
    //     // Just support one segment per request.
    //     if self.data_len > size {
    //         error!("More than one discard or write-zeroes segment is not supported");
    //         return iocompletecb.complete_request(VIRTIO_BLK_S_UNSUPP);
    //     }

    //     // Get and check the discard segment.
    //     let mut segment = DiscardWriteZeroesSeg::default();
    //     iov_to_buf_direct(&self.iovec, 0, segment.as_mut_bytes()).and_then(|v| {
    //         if v as u64 == size {
    //             Ok(())
    //         } else {
    //             error!("Invalid discard segment size {}", v);
    //             Err(HyperError::BadState)
    //         }
    //     })?;
    //     let sector = LittleEndian::read_u64(segment.sector.as_bytes());
    //     let num_sectors = LittleEndian::read_u32(segment.num_sectors.as_bytes());
    //     if sector
    //         .checked_add(num_sectors as u64)
    //         .filter(|&off| off <= iohandler.disk_sectors)
    //         .is_none()
    //         || num_sectors > MAX_REQUEST_SECTORS
    //     {
    //         error!(
    //             "Invalid discard or write zeroes request, sector offset {}, num_sectors {}",
    //             sector, num_sectors
    //         );
    //         return iocompletecb.complete_request(VIRTIO_BLK_S_IOERR);
    //     }
    //     let flags = LittleEndian::read_u32(segment.flags.as_bytes());
    //     if flags & !VIRTIO_BLK_WRITE_ZEROES_FLAG_UNMAP != 0 {
    //         error!("Invalid unmap flags 0x{:x}", flags);
    //         return iocompletecb.complete_request(VIRTIO_BLK_S_UNSUPP);
    //     }

    //     // The block_backend is not None here.
    //     let block_backend = iohandler.block_backend.as_ref().unwrap();
    //     let mut locked_backend = block_backend.lock().unwrap();
    //     let offset = (sector as usize) << SECTOR_SHIFT;
    //     let nbytes = (num_sectors as u64) << SECTOR_SHIFT;
    //     // trace::virtio_blk_handle_discard_write_zeroes_req(&opcode, flags, offset, nbytes);
    //     if opcode == OpCode::Discard {
    //         if flags == VIRTIO_BLK_WRITE_ZEROES_FLAG_UNMAP {
    //             error!("Discard request must not set unmap flags");
    //             return iocompletecb.complete_request(VIRTIO_BLK_S_UNSUPP);
    //         }
    //         locked_backend
    //             .discard(offset, nbytes, iocompletecb)
    //             .with_context(|| "Failed to process block request for discard")?;
    //     } else if opcode == OpCode::WriteZeroes {
    //         let unmap = flags == VIRTIO_BLK_WRITE_ZEROES_FLAG_UNMAP && iohandler.discard;
    //         locked_backend
    //             .write_zeroes(offset, nbytes, iocompletecb, unmap)
    //             .with_context(|| "Failed to process block request for write-zeroes")?;
    //     }
    //     Ok(())
    // }

    fn io_range_valid(&self, disk_sectors: u64) -> bool {
        match self.out_header.request_type {
            VIRTIO_BLK_T_IN | VIRTIO_BLK_T_OUT => {
                if self.data_len % SECTOR_SIZE != 0 {
                    error!("Failed to process block request with size not aligned to 512B");
                    return false;
                }
                if self
                    .get_req_sector_num()
                    .checked_add(self.out_header.sector)
                    .filter(|&off| off <= disk_sectors)
                    .is_none()
                {
                    error!(
                        "offset {} invalid, disk sector {}",
                        self.out_header.sector, disk_sectors
                    );
                    return false;
                }
                true
            }
            _ => true,
        }
    }

    fn get_req_sector_num(&self) -> u64 {
        self.data_len / SECTOR_SIZE
    }
}

/// Control block of Block IO.
struct BlockIoHandler {
    /// The virtqueue.
    queue: Arc<Mutex<Queue>>,
    // /// Eventfd of the virtqueue for IO event.
    // queue_evt: Arc<EventFd>,
    /// The address space to which the block device belongs.
    mem_space: Arc<AddressSpace>,
    // /// The block backend opened by the block device.
    // block_backend: Option<Arc<Mutex<dyn BlockDriverOps<AioCompleteCb>>>>,
    /// The align requirement of request(offset/len).
    req_align: u32,
    /// The align requirement of buffer(iova_base).
    buf_align: u32,
    /// The number of sectors of the disk image.
    disk_sectors: u64,
    /// Serial number of the block device.
    serial_num: Option<String>,
    /// If use direct access io.
    direct: bool,
    /// Bit mask of features negotiated by the backend and the frontend.
    driver_features: u64,
    // /// The receiving half of Rust's channel to receive the image file.
    // receiver: Receiver<SenderConfig>,
    // /// Eventfd for config space update.
    // update_evt: Arc<EventFd>,
    /// Device is broken or not.
    device_broken: Arc<AtomicBool>,
    /// Callback to trigger an interrupt.
    interrupt_cb: Arc<VirtioInterrupt>,
    // /// thread name of io handler
    // iothread: Option<String>,
    // /// Using the leak bucket to implement IO limits
    // leak_bucket: Option<LeakBucket>,
    // /// Supporting discard or not.
    // discard: bool,
    // /// The write-zeroes state.
    // write_zeroes: WriteZeroesState,
}

impl BlockIoHandler {
    fn process_queue_internal(&mut self) -> Result<bool> {
        let queue_size = self.queue.lock().vring.actual_size() as usize;
        let mut req_queue = Vec::with_capacity(queue_size);
        let mut done = false;

        loop {
            let mut queue = self.queue.lock();
            let mut elem = queue
                .vring
                .pop_avail(&self.mem_space, self.driver_features)?;
            if elem.desc_num == 0 {
                break;
            }
            // Init and put valid request into request queue.
            let mut status = VIRTIO_BLK_S_OK;
            let req = Request::new(self, &mut elem, &mut status)?;
            if status != VIRTIO_BLK_S_OK {
                // let aiocompletecb = AioCompleteCb::new(
                //     self.queue.clone(),
                //     self.mem_space.clone(),
                //     Arc::new(req),
                //     self.interrupt_cb.clone(),
                //     self.driver_features,
                // );
                // unlock queue, because it will be hold below.
                drop(queue);
                // aiocompletecb.complete_request(status)?;
                continue;
            }
            // Avoid bogus guest stuck IO thread.
            if req_queue.len() >= queue_size {
                error!("The front driver may be damaged, avail requests more than queue size");
                return Err(HyperError::BadState);
            }
            req_queue.push(req);
            done = true;
        }

        if req_queue.is_empty() {
            return Ok(done);
        }

        // let merge_req_queue = self.merge_req_queue(req_queue);
        // for req in merge_req_queue.into_iter() {
        //     let req_rc = Arc::new(req);
        //     let aiocompletecb = AioCompleteCb::new(
        //         self.queue.clone(),
        //         self.mem_space.clone(),
        //         req_rc.clone(),
        //         self.interrupt_cb.clone(),
        //         self.driver_features,
        //     );
        //     if let Some(block_backend) = self.block_backend.as_ref() {
        //         req_rc.execute(self, block_backend.clone(), aiocompletecb)?;
        //     } else {
        //         warn!("Failed to execute block request, block_backend not specified");
        //         aiocompletecb.complete_request(VIRTIO_BLK_S_IOERR)?;
        //     }
        // }
        // if let Some(block_backend) = self.block_backend.as_ref() {
        //     block_backend.lock().unwrap().flush_request()?;
        // }
        Ok(done)
    }

    fn process_queue_suppress_notify(&mut self) -> Result<bool> {
        // Note: locked_status has two function:
        // 1) set the status of the block device.
        // 2) as a mutex lock which is mutual exclusive with snapshot operations.
        // Do not unlock or drop the locked_status in this function.
        // let status;
        // let mut locked_status;
        let len = self.queue.lock().vring.avail_ring_len(&self.mem_space)?;

        let mut done = false;
        let mut iteration = 0;

        while self.queue.lock().vring.avail_ring_len(&self.mem_space)? != 0 {
            // Do not stuck IO thread.
            iteration += 1;
            // if iteration > MAX_ITERATION_PROCESS_QUEUE {
            //     // Make sure we can come back.
            //     // self.queue_evt.write(1)?;
            //     break;
            // }

            self.queue.lock().vring.suppress_queue_notify(
                &self.mem_space,
                self.driver_features,
                true,
            )?;

            done = self.process_queue_internal()?;

            self.queue.lock().vring.suppress_queue_notify(
                &self.mem_space,
                self.driver_features,
                false,
            )?;

            // See whether we have been throttled.
            // if let Some(lb) = self.leak_bucket.as_mut() {
            //     if let Some(ctx) = EventLoop::get_ctx(self.iothread.as_ref()) {
            //         if lb.throttled(ctx, 0) {
            //             break;
            //         }
            //     }
            // }
        }
        Ok(done)
    }

    fn process_queue(&mut self) -> Result<bool> {
        let result = self.process_queue_suppress_notify();
        if result.is_err() {
            report_virtio_error(
                self.interrupt_cb.clone(),
                self.driver_features,
                &self.device_broken,
            );
        }
        result
    }
}

#[repr(C, packed)]
#[derive(Copy, Clone, Debug, Default)]
struct VirtioBlkGeometry {
    cylinders: u16,
    heads: u8,
    sectors: u8,
}

impl ByteCode for VirtioBlkGeometry {}

#[repr(C, packed)]
#[derive(Copy, Clone, Debug, Default)]
pub struct VirtioBlkConfig {
    /// The capacity in 512 byte sectors.
    capacity: u64,
    /// The maximum segment size.
    size_max: u32,
    /// The maximum number of segments.
    pub seg_max: u32,
    /// Geometry of the block device.
    geometry: VirtioBlkGeometry,
    /// Block size of device.
    blk_size: u32,
    /// Exponent for physical block per logical block.
    physical_block_exp: u8,
    /// Alignment offset in logical blocks.
    alignment_offset: u8,
    /// Minimum I/O size without performance penalty in logical blocks.
    min_io_size: u16,
    /// Optimal sustained I/O size in logical blocks.
    opt_io_size: u32,
    /// Writeback mode.
    wce: u8,
    /// Reserved data.
    unused: u8,
    /// Number of virtio queues, only available when `VIRTIO_BLK_F_MQ` is set.
    pub num_queues: u16,
    /// The maximum discard sectors for one segment.
    pub max_discard_sectors: u32,
    /// The maximum number of discard segments in a discard command.
    pub max_discard_seg: u32,
    /// Discard commands must be aligned to this number of sectors.
    pub discard_sector_alignment: u32,
    /// The maximum number of write zeros sectors.
    pub max_write_zeroes_sectors: u32,
    /// The maximum number of segments in a write zeroes command.
    pub max_write_zeroes_seg: u32,
    /// Deallocation of one or more of the sectors.
    pub write_zeroes_may_unmap: u8,
    /// Reserved data.
    unused1: [u8; 3],
}

impl ByteCode for VirtioBlkConfig {}

/// State of block device.
#[repr(C)]
#[derive(Clone, Copy)]
// #[derive(Clone, Copy, Desc, ByteCode)]
// #[desc_version(compat_version = "0.1.0")]
pub struct BlockState {
    /// Bitmask of features supported by the backend.
    pub device_features: u64,
    /// Bit mask of features negotiated by the backend and the frontend.
    pub driver_features: u64,
    /// Config space of the block device.
    pub config_space: VirtioBlkConfig,
    /// Device broken status.
    broken: bool,
}

// #[derive(Debug, Clone, Serialize, Deserialize)]
#[derive(Debug, Clone)]
// #[serde(deny_unknown_fields)]
pub struct BlkDevConfig {
    pub id: String,
    pub path_on_host: String,
    pub read_only: bool,
    pub direct: bool,
    pub serial_num: Option<String>,
    pub iothread: Option<String>,
    pub iops: Option<u64>,
    pub queues: u16,
    pub boot_index: Option<u8>,
    pub chardev: Option<String>,
    pub socket_path: Option<String>,
    pub queue_size: u16,
    pub discard: bool,
    pub write_zeroes: WriteZeroesState,
    // pub format: DiskFormat,
    pub l2_cache_size: Option<u64>,
    pub refcount_cache_size: Option<u64>,
}

impl Default for BlkDevConfig {
    fn default() -> Self {
        BlkDevConfig {
            id: "".to_string(),
            path_on_host: "".to_string(),
            read_only: false,
            direct: true,
            serial_num: None,
            iothread: None,
            iops: None,
            queues: 1,
            boot_index: None,
            chardev: None,
            socket_path: None,
            queue_size: super::super::queue::DEFAULT_VIRTQUEUE_SIZE,
            discard: false,
            write_zeroes: WriteZeroesState::Off,
            // format: DiskFormat::Raw,
            l2_cache_size: None,
            refcount_cache_size: None,
        }
    }
}

/// Block device structure.
#[derive(Default)]
pub struct Block {
    /// Virtio device base property.
    base: VirtioBase,
    /// Configuration of the block device.
    blk_cfg: BlkDevConfig,
    /// Config space of the block device.
    config_space: VirtioBlkConfig,
    /// BLock backend opened by the block device.
    // block_backend: Option<Arc<Mutex<dyn BlockDriverOps<AioCompleteCb>>>>,
    /// The align requirement of request(offset/len).
    pub req_align: u32,
    /// The align requirement of buffer(iova_base).
    pub buf_align: u32,
    /// Number of sectors of the image file.
    disk_sectors: u64,
    /// Callback to trigger interrupt.
    interrupt_cb: Option<Arc<VirtioInterrupt>>,

    io_hanlders: Vec<BlockIoHandler>,
    // The sending half of Rust's channel to send the image file.
    // senders: Vec<Sender<SenderConfig>>,
    // Eventfd for config space update.
    // update_evts: Vec<Arc<EventFd>>,
    // Drive backend files.
    // drive_files: Arc<Mutex<HashMap<String, DriveFile>>>,
}

impl Block {
    pub fn new(
        blk_cfg: BlkDevConfig,
        // drive_files: Arc<Mutex<HashMap<String, DriveFile>>>,
    ) -> Block {
        let queue_num = blk_cfg.queues as usize;
        let queue_size = blk_cfg.queue_size;
        Self {
            base: VirtioBase::new(VIRTIO_TYPE_BLOCK, queue_num, queue_size),
            blk_cfg,
            req_align: 1,
            buf_align: 1,
            // Todo: generage from real Disk img file.
            disk_sectors: 131072, // 65536 KB
            // drive_files,
            ..Default::default()
        }
    }

    fn build_device_config_space(&mut self) {
        // capacity: 64bits
        self.config_space.capacity = self.disk_sectors;
        // seg_max = queue_size - 2: 32bits
        self.config_space.seg_max = self.queue_size_max() as u32 - 2;

        if self.blk_cfg.queues > 1 {
            self.config_space.num_queues = self.blk_cfg.queues;
        }

        if self.blk_cfg.discard {
            // Just support one segment per request.
            self.config_space.max_discard_seg = 1;
            // The default discard alignment is 1 sector.
            self.config_space.discard_sector_alignment = 1;
            self.config_space.max_discard_sectors = MAX_REQUEST_SECTORS;
        }

        if self.blk_cfg.write_zeroes != WriteZeroesState::Off {
            // Just support one segment per request.
            self.config_space.max_write_zeroes_seg = 1;
            self.config_space.max_write_zeroes_sectors = MAX_REQUEST_SECTORS;
            self.config_space.write_zeroes_may_unmap = 1;
        }
    }

    fn get_blk_config_size(&self) -> usize {
        use crate::device::virtio::{VIRTIO_BLK_F_DISCARD, VIRTIO_BLK_F_WRITE_ZEROES};
        if virtio_has_feature(self.base.device_features, VIRTIO_BLK_F_WRITE_ZEROES) {
            offset_of!(VirtioBlkConfig, unused1)
        } else if virtio_has_feature(self.base.device_features, VIRTIO_BLK_F_DISCARD) {
            offset_of!(VirtioBlkConfig, max_write_zeroes_sectors)
        } else {
            offset_of!(VirtioBlkConfig, max_discard_sectors)
        }
    }

    // fn gen_error_cb(&self, interrupt_cb: Arc<VirtioInterrupt>) -> BlockIoErrorCallback {
    //     let cloned_features = self.base.driver_features;
    //     let clone_broken = self.base.broken.clone();
    //     Arc::new(move || {
    //         report_virtio_error(interrupt_cb.clone(), cloned_features, &clone_broken);
    //     })
    // }
}

impl VirtioDevice for Block {
    fn virtio_base(&self) -> &VirtioBase {
        &self.base
    }

    fn virtio_base_mut(&mut self) -> &mut VirtioBase {
        &mut self.base
    }

    fn realize(&mut self) -> Result<()> {
        self.init_config_features()?;

        Ok(())
    }

    fn init_config_features(&mut self) -> Result<()> {
        let mut device_features = BlkFeature::VERSION_1
            .union(BlkFeature::RING_INDIRECT_DESC)
            .union(BlkFeature::RING_EVENT_IDX)
            .union(BlkFeature::FLUSH)
            .union(BlkFeature::SEG_MAX);

        if self.blk_cfg.read_only {
            device_features.set(BlkFeature::RO, true);
        };
        if self.blk_cfg.queues > 1 {
            device_features.set(BlkFeature::MQ, true);
        }
        if self.blk_cfg.discard {
            device_features.set(BlkFeature::DISCARD, true);
        }
        if self.blk_cfg.write_zeroes != WriteZeroesState::Off {
            device_features.set(BlkFeature::WRITE_ZEROES, true);
        }

        self.base.device_features = device_features.bits();
        self.build_device_config_space();

        Ok(())
    }

    // fn unrealize(&mut self) -> Result<()> {
    //     MigrationManager::unregister_device_instance(BlockState::descriptor(), &self.blk_cfg.id);
    //     let drive_files = self.drive_files.lock().unwrap();
    //     let drive_id = VmConfig::get_drive_id(&drive_files, &self.blk_cfg.path_on_host)?;
    //     remove_block_backend(&drive_id);
    //     Ok(())
    // }

    fn read_config(&self, offset: u64, data: &mut [u8]) -> Result<()> {
        let config_len = self.get_blk_config_size();
        let config = &self.config_space.as_bytes()[..config_len];
        read_config_default(config, offset, data)?;
        // trace::virtio_blk_read_config(offset, data);

        debug!(
            "Virtio-Block read_config at {:#x} get date {:x?}",
            offset, data
        );
        Ok(())
    }

    fn write_config(&mut self, offset: u64, data: &[u8]) -> Result<()> {
        debug!(
            "Virtio-Block write_config at {:#x}, data {:x?}",
            offset, data
        );
        let config_len = self.get_blk_config_size();
        let config = &self.config_space.as_bytes()[..config_len];
        check_config_space_rw(config, offset, data)?;
        Ok(())
    }

    fn activate(
        &mut self,
        mem_space: Arc<AddressSpace>,
        interrupt_cb: Arc<VirtioInterrupt>,
    ) -> Result<()> {
        info!("Activating Virtio Block {}", self.blk_cfg.id);
        self.interrupt_cb = Some(interrupt_cb.clone());
        let queues = self.base.queues.clone();
        for (index, queue) in queues.iter().enumerate() {
            if !queue.lock().is_enabled() {
                warn!(
                    "Virtio Block {} queue {} is not enabled",
                    self.blk_cfg.id, index
                );
                continue;
            }
            let handler = BlockIoHandler {
                queue: queue.clone(),
                mem_space: mem_space.clone(),
                req_align: self.req_align,
                buf_align: self.buf_align,
                disk_sectors: self.disk_sectors,
                serial_num: self.blk_cfg.serial_num.clone(),
                direct: self.blk_cfg.direct,
                driver_features: self.base.driver_features,
                device_broken: self.base.broken.clone(),
                interrupt_cb: interrupt_cb.clone(),
            };
            self.io_hanlders.push(handler)
        }

        Ok(())
    }

    fn update_config(&mut self) -> Result<()> {
        error!("Unsupported to update configuration");
        Err(HyperError::BadState)
    }

    fn notify_handler(&mut self, val: u16) -> Result {
        self.io_hanlders[val as usize].process_queue()?;
        Ok(())
    }
}

// SAFETY: Send and Sync is not auto-implemented for `Sender` type.
// Implementing them is safe because `Sender` field of Block won't
// change in migration workflow.
unsafe impl Sync for Block {}
