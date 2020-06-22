#![allow(dead_code)]
use byteorder::{NetworkEndian, ReadBytesExt, WriteBytesExt};
use nix::{errno::Errno, libc::ioctl, request_code_none};
use std::io::Cursor;
use std::io::{Error, ErrorKind};
use std::os::unix::io::{AsRawFd, RawFd};

const CMD_MASK_COMMAND: u32 = 0x0000_ffff;
const REQUEST_MAGIC: u32 = 0x2560_9513;
const REPLY_MAGIC: u32 = 0x6744_6698;

pub const SIZE_OF_REQUEST: usize = 28;
pub const SIZE_OF_REPLY: usize = 16;

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

pub fn set_sock<F>(f: &F, sock: RawFd) -> Result<i32, Error>
where
    F: AsRawFd,
{
    Errno::result(unsafe { ioctl(f.as_raw_fd(), request_code_none!(0xab, 0), sock) })
        .map_err(errno_to_io)
}

pub fn set_block_size<F>(f: &F, size: u32) -> Result<i32, Error>
where
    F: AsRawFd,
{
    Errno::result(unsafe { ioctl(f.as_raw_fd(), request_code_none!(0xab, 1), size) })
        .map_err(errno_to_io)
}

pub fn do_it<F>(f: &F) -> Result<i32, Error>
where
    F: AsRawFd,
{
    Errno::result(unsafe { ioctl(f.as_raw_fd(), request_code_none!(0xab, 3)) }).map_err(errno_to_io)
}

pub fn clear_sock<F>(f: &F) -> Result<i32, Error>
where
    F: AsRawFd,
{
    Errno::result(unsafe { ioctl(f.as_raw_fd(), request_code_none!(0xab, 4)) }).map_err(errno_to_io)
}

pub fn clear_queue<F>(f: &F) -> Result<i32, Error>
where
    F: AsRawFd,
{
    Errno::result(unsafe { ioctl(f.as_raw_fd(), request_code_none!(0xab, 5)) }).map_err(errno_to_io)
}

pub fn set_size_blocks<F>(f: &F, size: u64) -> Result<i32, Error>
where
    F: AsRawFd,
{
    Errno::result(unsafe { ioctl(f.as_raw_fd(), request_code_none!(0xab, 7), size) })
        .map_err(errno_to_io)
}

pub fn disconnect<F>(f: &F) -> Result<i32, Error>
where
    F: AsRawFd,
{
    Errno::result(unsafe { ioctl(f.as_raw_fd(), request_code_none!(0xab, 8)) }).map_err(errno_to_io)
}

pub fn set_timeout<F>(f: &F, timeout: u64) -> Result<i32, Error>
where
    F: AsRawFd,
{
    Errno::result(unsafe { ioctl(f.as_raw_fd(), request_code_none!(0xab, 9), timeout) })
        .map_err(errno_to_io)
}

pub fn set_flags<F>(f: &F, flags: u64) -> Result<i32, Error>
where
    F: AsRawFd,
{
    Errno::result(unsafe { ioctl(f.as_raw_fd(), request_code_none!(0xab, 10), flags) })
        .map_err(errno_to_io)
}

fn errno_to_io(error: nix::Error) -> Error {
    match error {
        nix::Error::Sys(errno) => Error::from_raw_os_error(errno as i32),
        nix::Error::InvalidPath => Error::from(ErrorKind::InvalidInput),
        nix::Error::InvalidUtf8 => Error::from(ErrorKind::InvalidData),
        nix::Error::UnsupportedOperation => Error::new(ErrorKind::Other, "not supported"),
    }
}

#[derive(Clone, Copy, Debug)]
pub enum Command {
    Read,
    Write,
    Disc,
    Flush,
    Trim,
    WriteZeroes,
}

#[derive(Debug, Clone)]
pub struct RequestFlags {
    // Force Unit Access
    fua: bool,
    no_hole: bool,
}

#[derive(Debug, Clone)]
pub struct Request {
    pub magic: u32,
    pub command: Command,
    pub flags: RequestFlags,
    pub handle: u64,
    pub from: u64,
    pub len: usize,
}

impl Request {
    pub fn try_from_bytes(d: &[u8]) -> Result<Self, Error> {
        let mut rdr = Cursor::new(d);
        let magic = rdr.read_u32::<NetworkEndian>()?;
        let type_f = rdr.read_u32::<NetworkEndian>()?;
        let handle = rdr.read_u64::<NetworkEndian>()?;
        let from = rdr.read_u64::<NetworkEndian>()?;
        let len = rdr.read_u32::<NetworkEndian>()? as usize;
        if magic != REQUEST_MAGIC {
            return Err(Error::new(ErrorKind::InvalidData, "invalid magic"));
        }
        let command = match type_f & CMD_MASK_COMMAND {
            0 => Command::Read,
            1 => Command::Write,
            2 => Command::Disc,
            3 => Command::Flush,
            4 => Command::Trim,
            5 => Command::WriteZeroes,
            _ => return Err(Error::new(ErrorKind::InvalidData, "invalid command")),
        };
        let flags = type_f >> 16;
        let fua = flags & 1 == 1;
        let no_hole = flags & (1 << 1) == (1 << 1);
        Ok(Self {
            magic,
            command,
            flags: RequestFlags { fua, no_hole },
            handle,
            from,
            len,
        })
    }
}

#[derive(Debug, Clone)]
pub struct Reply {
    pub magic: u32,
    pub error: i32,
    pub handle: u64,
}

impl Reply {
    pub fn from_request(request: &Request) -> Self {
        Self {
            magic: REPLY_MAGIC,
            handle: request.handle,
            error: 0,
        }
    }
    pub fn append_to_vec(&self, buf: &mut Vec<u8>) -> Result<(), Error> {
        buf.write_u32::<NetworkEndian>(self.magic)?;
        buf.write_i32::<NetworkEndian>(self.error)?;
        buf.write_u64::<NetworkEndian>(self.handle)?;
        Ok(())
    }
    pub fn write_to_slice(&self, mut slice: &mut [u8]) -> Result<(), Error> {
        slice.write_u32::<NetworkEndian>(self.magic)?;
        slice.write_i32::<NetworkEndian>(self.error)?;
        slice.write_u64::<NetworkEndian>(self.handle)?;
        Ok(())
    }
}
