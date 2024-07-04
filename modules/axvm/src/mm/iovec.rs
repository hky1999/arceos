use alloc::vec::Vec;
use core::cmp;

use crate::Result;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Iovec {
    pub iov_base: u64,
    pub iov_len: u64,
}

impl Iovec {
    pub fn new(base: u64, len: u64) -> Self {
        Iovec {
            iov_base: base,
            iov_len: len,
        }
    }

    pub fn is_none(&self) -> bool {
        self.iov_base == 0 && self.iov_len == 0
    }
}

pub fn get_iov_size(iovecs: &[Iovec]) -> u64 {
    let mut sum = 0;
    for iov in iovecs {
        sum += iov.iov_len;
    }
    sum
}

pub fn mem_from_buf(buf: &[u8], hva: u64) -> Result<()> {
    // SAFETY: all callers have valid hva address.
    let mut slice = unsafe { core::slice::from_raw_parts_mut(hva as *mut u8, buf.len()) };
    // (&mut slice)
    //     .write(buf)
    //     .with_context(|| format!("Failed to write buf to hva:{})", hva))?;
    (&mut slice).copy_from_slice(buf);
    Ok(())
}

/// Write buf to iovec and return the written number of bytes.
pub fn iov_from_buf_direct(iovec: &[Iovec], buf: &[u8]) -> Result<usize> {
    let mut start: usize = 0;
    let mut end: usize = 0;

    for iov in iovec.iter() {
        end = cmp::min(start + iov.iov_len as usize, buf.len());
        mem_from_buf(&buf[start..end], iov.iov_base)?;
        if end >= buf.len() {
            break;
        }
        start = end;
    }
    Ok(end)
}

pub fn mem_to_buf(mut buf: &mut [u8], hva: u64) -> Result<()> {
    // SAFETY: all callers have valid hva address.
    let slice = unsafe { core::slice::from_raw_parts(hva as *const u8, buf.len()) };
    // buf.write(slice)
    //     .with_context(|| format!("Failed to read buf from hva:{})", hva))?;
    buf.copy_from_slice(slice);
   
    Ok(())
}

/// Read iovec to buf and return the read number of bytes.
pub fn iov_to_buf_direct(iovec: &[Iovec], offset: u64, buf: &mut [u8]) -> Result<usize> {
    let mut iovec2: Option<&[Iovec]> = None;
    let mut start: usize = 0;
    let mut end: usize = 0;

    if offset == 0 {
        iovec2 = Some(iovec);
    } else {
        let mut offset = offset;
        for (index, iov) in iovec.iter().enumerate() {
            if iov.iov_len > offset {
                end = cmp::min((iov.iov_len - offset) as usize, buf.len());
                mem_to_buf(&mut buf[..end], iov.iov_base + offset)?;
                if end >= buf.len() || index >= (iovec.len() - 1) {
                    return Ok(end);
                }
                start = end;
                iovec2 = Some(&iovec[index + 1..]);
                break;
            }
            offset -= iov.iov_len;
        }
        if iovec2.is_none() {
            return Ok(0);
        }
    }

    for iov in iovec2.unwrap() {
        end = cmp::min(start + iov.iov_len as usize, buf.len());
        mem_to_buf(&mut buf[start..end], iov.iov_base)?;
        if end >= buf.len() {
            break;
        }
        start = end;
    }
    Ok(end)
}

/// Discard "size" bytes of the front of iovec.
pub fn iov_discard_front_direct(iovec: &mut [Iovec], mut size: u64) -> Option<&mut [Iovec]> {
    for (index, iov) in iovec.iter_mut().enumerate() {
        if iov.iov_len > size {
            iov.iov_base += size;
            iov.iov_len -= size;
            return Some(&mut iovec[index..]);
        }
        size -= iov.iov_len;
    }
    None
}

fn iovec_is_zero(iovecs: &[Iovec]) -> bool {
    let size = core::mem::size_of::<u64>() as u64;
    for iov in iovecs {
        if iov.iov_len % size != 0 {
            return false;
        }
        // SAFETY: iov_base and iov_len has been checked in pop_avail().
        let slice = unsafe {
            core::slice::from_raw_parts(iov.iov_base as *const u64, (iov.iov_len / size) as usize)
        };
        for val in slice.iter() {
            if *val != 0 {
                return false;
            }
        }
    }
    true
}

pub fn iovecs_split(iovecs: Vec<Iovec>, mut size: u64) -> (Vec<Iovec>, Vec<Iovec>) {
    let mut begin = Vec::new();
    let mut end = Vec::new();
    for iov in iovecs {
        if size == 0 {
            end.push(iov);
            continue;
        }
        if iov.iov_len > size {
            begin.push(Iovec::new(iov.iov_base, size));
            end.push(Iovec::new(iov.iov_base + size, iov.iov_len - size));
            size = 0;
        } else {
            size -= iov.iov_len;
            begin.push(iov);
        }
    }
    (begin, end)
}

pub fn iovec_write_zero(iovec: &[Iovec]) {
    for iov in iovec.iter() {
        // SAFETY: all callers have valid hva address.
        unsafe {
            core::ptr::write_bytes(iov.iov_base as *mut u8, 0, iov.iov_len as usize);
        }
    }
}
