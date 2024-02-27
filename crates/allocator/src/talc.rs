//! The Talc Allocator memory allocation.

// use talc::Talc;

use talc::*;

use core::alloc::Layout;
use core::ptr::NonNull;

use crate::{AllocError, AllocResult, BaseAllocator, ByteAllocator};

/// Talc Allocator.
///
/// [talc]: https://docs.rs/talc/4.2.0/talc/
pub struct TalcByteAllocator {
    inner: Talc<ErrOnOom>,
    total_bytes: usize,
    used_bytes: usize,
}

impl TalcByteAllocator {
    /// Creates a new empty `TalcByteAllocator`.
    pub const fn new() -> Self {
        Self {
            inner: Talc::new(ErrOnOom),
            total_bytes: 0,
            used_bytes: 0,
        }
    }
}

impl BaseAllocator for TalcByteAllocator {
    fn init(&mut self, start: usize, size: usize) {
        unsafe {
            self.inner
                .claim(Span::from_base_size(start as *mut u8, size))
                .expect("TalcByteAllocator init failed");
        }
        self.total_bytes = size;
    }

    fn add_memory(&mut self, start: usize, size: usize) -> AllocResult {
        // self.inner.extend(by)
        unsafe {
            self.inner
                .claim(Span::from_base_size(start as *mut u8, size))
                .expect("TalcByteAllocator add_memory failed");
        }
        self.total_bytes += size;
        // self.inner.extend(old_heap, req_heap)
        // unsafe { self.inner.add_to_heap(start, start + size) };
        Ok(())
    }
}

impl ByteAllocator for TalcByteAllocator {
    fn alloc(&mut self, layout: Layout) -> AllocResult<NonNull<u8>> {
        let ptr = unsafe { self.inner.malloc(layout) }.map_err(|_| AllocError::NoMemory)?;
        self.used_bytes += layout.size();
        Ok(ptr)
    }

    fn dealloc(&mut self, pos: NonNull<u8>, layout: Layout) {
        unsafe { self.inner.free(pos, layout) }
        self.used_bytes -= layout.size();
    }

    fn total_bytes(&self) -> usize {
        self.total_bytes
    }

    fn used_bytes(&self) -> usize {
        self.used_bytes
    }

    fn available_bytes(&self) -> usize {
        self.total_bytes - self.used_bytes
    }
}
