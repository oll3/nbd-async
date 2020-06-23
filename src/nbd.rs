use byteorder::{NetworkEndian, ReadBytesExt, WriteBytesExt};
use std::io::{self, Cursor};

pub const CMD_MASK_COMMAND: u32 = 0x0000_ffff;
pub const REQUEST_MAGIC: u32 = 0x2560_9513;
pub const REPLY_MAGIC: u32 = 0x6744_6698;

pub const SIZE_OF_REQUEST: usize = 28;
pub const SIZE_OF_REPLY: usize = 16;

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
    pub fn try_from_bytes(d: &[u8]) -> io::Result<Self> {
        let mut rdr = Cursor::new(d);
        let magic = rdr.read_u32::<NetworkEndian>()?;
        let type_f = rdr.read_u32::<NetworkEndian>()?;
        let handle = rdr.read_u64::<NetworkEndian>()?;
        let from = rdr.read_u64::<NetworkEndian>()?;
        let len = rdr.read_u32::<NetworkEndian>()? as usize;
        if magic != REQUEST_MAGIC {
            return Err(io::Error::new(io::ErrorKind::InvalidData, "invalid magic"));
        }
        let command = match type_f & CMD_MASK_COMMAND {
            0 => Command::Read,
            1 => Command::Write,
            2 => Command::Disc,
            3 => Command::Flush,
            4 => Command::Trim,
            5 => Command::WriteZeroes,
            _ => return Err(io::Error::new(io::ErrorKind::InvalidData, "invalid command")),
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
    pub fn append_to_vec(&self, buf: &mut Vec<u8>) -> io::Result<()> {
        buf.write_u32::<NetworkEndian>(self.magic)?;
        buf.write_i32::<NetworkEndian>(self.error)?;
        buf.write_u64::<NetworkEndian>(self.handle)?;
        Ok(())
    }
    pub fn write_to_slice(&self, mut slice: &mut [u8]) -> io::Result<()> {
        slice.write_u32::<NetworkEndian>(self.magic)?;
        slice.write_i32::<NetworkEndian>(self.error)?;
        slice.write_u64::<NetworkEndian>(self.handle)?;
        Ok(())
    }
}
