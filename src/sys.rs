#![allow(dead_code)]
use std::io;
use std::os::unix::io::{AsRawFd, RawFd};

use nix::{errno::Errno, libc::ioctl, request_code_none};

// Flags are there
const HAS_FLAGS: u64 = 1;
// Device is read-only
const READ_ONLY: u64 = 1 << 1;
// Send FLUSH
const SEND_FLUSH: u64 = 1 << 2;
// Send FUA (Force Unit Access)
const SEND_FUA: u64 = 1 << 3;
// Use elevator algorithm - rotational media
const ROTATIONAL: u64 = 1 << 4;
// Send TRIM (discard)
const SEND_TRIM: u64 = 1 << 5;
// Send NBD_CMD_WRITE_ZEROES
const SEND_WRITE_ZEROES: u64 = 1 << 6;
// Multiple connections are okay
const CAN_MULTI_CONN: u64 = 1 << 8;

pub fn set_sock<F>(f: &F, sock: RawFd) -> io::Result<i32>
where
    F: AsRawFd,
{
    Errno::result(unsafe { ioctl(f.as_raw_fd(), request_code_none!(0xab, 0), sock) })
        .map_err(errno_to_io)
}

pub fn set_block_size<F>(f: &F, size: u32) -> io::Result<i32>
where
    F: AsRawFd,
{
    Errno::result(unsafe { ioctl(f.as_raw_fd(), request_code_none!(0xab, 1), size) })
        .map_err(errno_to_io)
}

pub fn do_it<F>(f: &F) -> io::Result<i32>
where
    F: AsRawFd,
{
    Errno::result(unsafe { ioctl(f.as_raw_fd(), request_code_none!(0xab, 3)) }).map_err(errno_to_io)
}

pub fn clear_sock<F>(f: &F) -> io::Result<i32>
where
    F: AsRawFd,
{
    Errno::result(unsafe { ioctl(f.as_raw_fd(), request_code_none!(0xab, 4)) }).map_err(errno_to_io)
}

pub fn clear_queue<F>(f: &F) -> io::Result<i32>
where
    F: AsRawFd,
{
    Errno::result(unsafe { ioctl(f.as_raw_fd(), request_code_none!(0xab, 5)) }).map_err(errno_to_io)
}

pub fn set_size_blocks<F>(f: &F, size: u64) -> io::Result<i32>
where
    F: AsRawFd,
{
    Errno::result(unsafe { ioctl(f.as_raw_fd(), request_code_none!(0xab, 7), size) })
        .map_err(errno_to_io)
}

pub fn disconnect<F>(f: &F) -> io::Result<i32>
where
    F: AsRawFd,
{
    Errno::result(unsafe { ioctl(f.as_raw_fd(), request_code_none!(0xab, 8)) }).map_err(errno_to_io)
}

pub fn set_timeout<F>(f: &F, timeout: u64) -> io::Result<i32>
where
    F: AsRawFd,
{
    Errno::result(unsafe { ioctl(f.as_raw_fd(), request_code_none!(0xab, 9), timeout) })
        .map_err(errno_to_io)
}

pub fn set_flags<F>(f: &F, flags: u64) -> io::Result<i32>
where
    F: AsRawFd,
{
    Errno::result(unsafe { ioctl(f.as_raw_fd(), request_code_none!(0xab, 10), flags) })
        .map_err(errno_to_io)
}

fn errno_to_io(error: nix::Error) -> io::Error {
    match error {
        nix::Error::Sys(errno) => io::Error::from_raw_os_error(errno as i32),
        nix::Error::InvalidPath => io::Error::from(io::ErrorKind::InvalidInput),
        nix::Error::InvalidUtf8 => io::Error::from(io::ErrorKind::InvalidData),
        nix::Error::UnsupportedOperation => io::Error::new(io::ErrorKind::Other, "not supported"),
    }
}
