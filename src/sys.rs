#![allow(dead_code)]
use std::io;
use std::os::unix::io::{AsRawFd, RawFd};

use nix::{errno::Errno, libc::ioctl, request_code_none};

// Flags are there
pub const HAS_FLAGS: u64 = 1;
// Device is read-only
pub const READ_ONLY: u64 = 1 << 1;
// Send FLUSH
pub const SEND_FLUSH: u64 = 1 << 2;
// Send FUA (Force Unit Access)
pub const SEND_FUA: u64 = 1 << 3;
// Use elevator algorithm - rotational media
pub const ROTATIONAL: u64 = 1 << 4;
// Send TRIM (discard)
pub const SEND_TRIM: u64 = 1 << 5;
// Send NBD_CMD_WRITE_ZEROES
pub const SEND_WRITE_ZEROES: u64 = 1 << 6;
// Multiple connections are okay
pub const CAN_MULTI_CONN: u64 = 1 << 8;

pub fn set_sock<F>(f: &F, sock: RawFd) -> io::Result<i32>
where
    F: AsRawFd,
{
    Ok(Errno::result(unsafe {
        ioctl(f.as_raw_fd(), request_code_none!(0xab, 0), sock)
    })?)
}

pub fn set_block_size<F>(f: &F, size: u32) -> io::Result<i32>
where
    F: AsRawFd,
{
    Ok(Errno::result(unsafe {
        ioctl(f.as_raw_fd(), request_code_none!(0xab, 1), size)
    })?)
}

pub fn do_it<F>(f: &F) -> io::Result<i32>
where
    F: AsRawFd,
{
    Ok(Errno::result(unsafe {
        ioctl(f.as_raw_fd(), request_code_none!(0xab, 3))
    })?)
}

pub fn clear_sock<F>(f: &F) -> io::Result<i32>
where
    F: AsRawFd,
{
    Ok(Errno::result(unsafe {
        ioctl(f.as_raw_fd(), request_code_none!(0xab, 4))
    })?)
}

pub fn clear_queue<F>(f: &F) -> io::Result<i32>
where
    F: AsRawFd,
{
    Ok(Errno::result(unsafe {
        ioctl(f.as_raw_fd(), request_code_none!(0xab, 5))
    })?)
}

pub fn set_size_blocks<F>(f: &F, size: u64) -> io::Result<i32>
where
    F: AsRawFd,
{
    Ok(Errno::result(unsafe {
        ioctl(f.as_raw_fd(), request_code_none!(0xab, 7), size)
    })?)
}

pub fn disconnect<F>(f: &F) -> io::Result<i32>
where
    F: AsRawFd,
{
    Ok(Errno::result(unsafe {
        ioctl(f.as_raw_fd(), request_code_none!(0xab, 8))
    })?)
}

pub fn set_timeout<F>(f: &F, timeout: u64) -> io::Result<i32>
where
    F: AsRawFd,
{
    Ok(Errno::result(unsafe {
        ioctl(f.as_raw_fd(), request_code_none!(0xab, 9), timeout)
    })?)
}

pub fn set_flags<F>(f: &F, flags: u64) -> io::Result<i32>
where
    F: AsRawFd,
{
    Ok(Errno::result(unsafe {
        ioctl(f.as_raw_fd(), request_code_none!(0xab, 10), flags)
    })?)
}
