use core::slice::{from_raw_parts, from_raw_parts_mut};
use core::sync::atomic::{AtomicBool, AtomicU64, Ordering};

use super::allocator::SyscallDataBuffer;
use super::queue::{ScfRequestToken, SyscallQueueBuffer};
use axmem::{UserInPtr, UserOutPtr};

use crate::syscall::Sysno;

const CHUNK_SIZE: usize = 256;

pub struct SyscallCondVar {
    ok: AtomicBool,
    ret_val: AtomicU64,
}

impl SyscallCondVar {
    pub fn new() -> Self {
        Self {
            ok: AtomicBool::new(false),
            ret_val: AtomicU64::new(0),
        }
    }

    pub fn signal(&self, ret_val: u64) {
        self.ret_val.store(ret_val, Ordering::Release);
        self.ok.store(true, Ordering::Release);
    }

    pub fn wait(&self) -> u64 {
        while !self.ok.load(Ordering::Acquire) {
            axtask::yield_now();
        }
        self.ret_val.load(Ordering::Acquire)
    }
}

/// Forwarded syscall args, does not contains syscall number.
#[repr(C)]
#[derive(Debug)]
struct SyscallArgs {
    args: [u64; 6],
}

fn send_request(opcode: Sysno, args_offset: u64, token: ScfRequestToken) {
    while !SyscallQueueBuffer::get().send(opcode, args_offset, token) {
        axtask::yield_now();
    }
    super::notify();
}

pub fn scf_write(fd: usize, buf: UserInPtr<u8>, len: usize) -> isize {
    debug!("scf write fd {} len {:#x}", fd, len);
    assert!(len < CHUNK_SIZE);
    let pool = SyscallDataBuffer::get();
    let chunk_ptr = unsafe { pool.alloc_array_uninit::<u8>(len) };
    buf.read_buf(unsafe { from_raw_parts_mut(chunk_ptr as _, len) });
    let args = pool.alloc(SyscallArgs {
        args: [fd as u64, pool.offset_of(chunk_ptr), len as u64, 0, 0, 0],
    });
    let cond = SyscallCondVar::new();
    send_request(
        Sysno::write,
        pool.offset_of(args),
        ScfRequestToken::from(&cond),
    );
    let ret = cond.wait();
    unsafe {
        pool.dealloc(chunk_ptr);
        pool.dealloc(args);
    }
    ret as _
}

pub fn scf_read(fd: usize, mut buf: UserOutPtr<u8>, len: usize) -> isize {
    assert!(len < CHUNK_SIZE);
    let pool = SyscallDataBuffer::get();
    let chunk_ptr = unsafe { pool.alloc_array_uninit::<u8>(len) };
    let args = pool.alloc(SyscallArgs {
        args: [fd as u64, pool.offset_of(chunk_ptr), len as u64, 0, 0, 0],
    });
    let cond = SyscallCondVar::new();
    send_request(
        Sysno::read,
        pool.offset_of(args),
        ScfRequestToken::from(&cond),
    );
    let ret = cond.wait();
    unsafe {
        buf.write_buf(from_raw_parts(chunk_ptr as _, len));
        pool.dealloc(chunk_ptr);
        pool.dealloc(args);
    }
    ret as _
}

pub fn scf_open_vdisk(id: usize) -> isize {
    let pool = SyscallDataBuffer::get();
    let args = pool.alloc(SyscallArgs {
        args: [id as u64, 0, 0, 0, 0, 0],
    });
    let cond = SyscallCondVar::new();
    send_request(
        0xfbu8 as Sysno,
        pool.offset_of(args),
        ScfRequestToken::from(&cond),
    );
    let ret = cond.wait();
    unsafe {
        pool.dealloc(args);
    }
    ret as _
}

pub const VDISK_BLOCK_SIZE: usize = 512;

pub fn scf_read_vdisk_block(id: usize, block: usize, dest: &mut[u8; VDISK_BLOCK_SIZE]) -> isize {
    let pool = SyscallDataBuffer::get();
    let chunk_ptr = unsafe { pool.alloc_array_uninit::<u8>(VDISK_BLOCK_SIZE) };
    let args = pool.alloc(SyscallArgs {
        args: [id as u64, block as u64, pool.offset_of(dest), 0, 0, 0],
    });
    let cond = SyscallCondVar::new();
    send_request(
        0xfcu8 as Sysno,
        pool.offset_of(chunk_ptr),
        ScfRequestToken::from(&cond),
    );
    let ret = cond.wait();
    unsafe {
        dest.copy_from_slice(from_raw_parts(chunk_ptr, VDISK_BLOCK_SIZE));
        pool.dealloc(chunk_ptr);
        pool.dealloc(args);
    }
    ret as _
}

pub fn scf_write_vdisk_block(id: usize, block: usize, src: &[u8; VDISK_BLOCK_SIZE]) -> isize {
    let pool = SyscallDataBuffer::get();
    let chunk_ptr = unsafe { pool.alloc_array_uninit::<u8>(VDISK_BLOCK_SIZE) };
    unsafe { from_raw_parts_mut(chunk_ptr, VDISK_BLOCK_SIZE).copy_from_slice(src); }
    let args = pool.alloc(SyscallArgs {
        args: [id as u64, block as u64, pool.offset_of(chunk_ptr), 0, 0, 0],
    });
    let cond = SyscallCondVar::new();
    send_request(
        0xfdu8 as Sysno,
        pool.offset_of(args),
        ScfRequestToken::from(&cond),
    );
    let ret = cond.wait();
    unsafe {
        pool.dealloc(chunk_ptr);
        pool.dealloc(args);
    }
    ret as _
}
