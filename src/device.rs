use async_trait::async_trait;
use core::pin::Pin;
use core::task::{Context, Poll};
use futures::stream::{Stream, StreamExt};
use std::io;
use std::os::unix::io::AsRawFd;
use std::path::Path;
use std::thread::JoinHandle;
use tokio::{fs::OpenOptions, io::AsyncRead, io::AsyncReadExt, io::AsyncWriteExt, net::UnixStream};

use crate::{nbd, sys};

/// A block device.
#[async_trait]
pub trait BlockDevice {
    /// Read a block from offset.
    async fn read(&mut self, offset: u64, buf: &mut [u8]) -> io::Result<()>;
    /// Write a block of data at offset.
    async fn write(&mut self, offset: u64, buf: &[u8]) -> io::Result<()>;
    /// Size of a block on device.
    fn block_size(&self) -> u32;
    /// Number of blocks on device.
    fn block_count(&self) -> u64;
}

struct RequestStream {
    sock: Option<UnixStream>,
    do_it_thread: Option<JoinHandle<io::Result<()>>>,
    read_buf: [u8; nbd::SIZE_OF_REQUEST],
    file: tokio::fs::File,
}

/// Attach a block device to a NBD dev file.
pub async fn attach_device<P, B>(path: P, mut block_device: B) -> io::Result<()>
where
    P: AsRef<Path>,
    B: Unpin + BlockDevice,
{
    let file = OpenOptions::new()
        .read(true)
        .write(true)
        .open(path.as_ref())
        .await?;

    let (sock, kernel_sock) = UnixStream::pair()?;
    sys::set_block_size(&file, block_device.block_size())?;
    sys::set_size_blocks(&file, block_device.block_count())?;
    sys::set_timeout(&file, 10)?;
    sys::clear_sock(&file)?;

    let inner_file = file.try_clone().await?;
    let do_it_thread = Some(std::thread::spawn(move || -> io::Result<()> {
        sys::set_sock(&inner_file, kernel_sock.as_raw_fd())?;
        let _ = sys::set_flags(&inner_file, 0);
        // The do_it ioctl will block until device is disconnected, hence
        // the separate thread.
        sys::do_it(&inner_file)?;
        let _ = sys::clear_sock(&inner_file);
        let _ = sys::clear_queue(&inner_file);
        Ok(())
    }));

    let mut stream = RequestStream {
        sock: Some(sock),
        do_it_thread,
        read_buf: [0; nbd::SIZE_OF_REQUEST],
        file,
    };

    let mut reply_buf = vec![];
    let mut write_buf = vec![];
    while let Some(result) = stream.next().await {
        let request = result?;
        let sock = match stream.sock {
            Some(ref mut sock) => sock,
            None => break,
        };
        let mut reply = nbd::Reply::from_request(&request);
        match request.command {
            nbd::Command::Read => {
                let start_offs = reply_buf.len();
                reply_buf.resize(start_offs + nbd::SIZE_OF_REPLY + request.len, 0);
                if let Err(err) = block_device
                    .read(
                        request.from,
                        &mut reply_buf[start_offs + nbd::SIZE_OF_REPLY..],
                    )
                    .await
                {
                    reply.error = err.raw_os_error().unwrap_or(nix::errno::Errno::EIO as i32);
                }
                reply.write_to_slice(&mut reply_buf[start_offs..])?;
            }
            nbd::Command::Write => {
                write_buf.resize(request.len, 0);
                sock.read_exact(&mut write_buf).await?;
                if let Err(err) = block_device.write(request.from, &write_buf[..]).await {
                    reply.error = err.raw_os_error().unwrap_or(nix::errno::Errno::EIO as i32);
                }
                reply.append_to_vec(&mut reply_buf)?;
            }
            nbd::Command::Flush => {
                reply.append_to_vec(&mut reply_buf)?;
            }
            nbd::Command::Disc => unimplemented!(),
            nbd::Command::Trim => unimplemented!(),
            nbd::Command::WriteZeroes => unimplemented!(),
        }
        sock.write_all(&reply_buf).await?;
        reply_buf.clear();
    }
    Ok(())
}

impl Drop for RequestStream {
    fn drop(&mut self) {
        let _ = sys::disconnect(&self.file);
        self.sock = None;
        if let Some(do_it_thread) = self.do_it_thread.take() {
            do_it_thread.join().expect("join thread").unwrap();
        }
    }
}

impl RequestStream {
    fn read_next(&mut self, cx: &mut Context) -> Poll<Option<io::Result<nbd::Request>>> {
        let sock = match self.sock {
            Some(ref mut sock) => sock,
            None => return Poll::Ready(None),
        };
        let read_buf = &mut self.read_buf;
        let rc = Pin::new(sock).poll_read(cx, read_buf);
        match rc {
            Poll::Ready(Ok(0)) => return Poll::Ready(None),
            Poll::Ready(Ok(n)) if n != nbd::SIZE_OF_REQUEST => {
                return Poll::Ready(Some(Err(io::Error::from(io::ErrorKind::UnexpectedEof))));
            }
            Poll::Ready(Ok(n)) => n,
            Poll::Ready(Err(err)) => return Poll::Ready(Some(Err(err))),
            Poll::Pending => {
                return Poll::Pending;
            }
        };
        Poll::Ready(Some(match nbd::Request::try_from_bytes(&self.read_buf) {
            Ok(req) => Ok(req),
            Err(err) => Err(err),
        }))
    }
}

impl Stream for RequestStream {
    type Item = io::Result<nbd::Request>;
    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context) -> Poll<Option<Self::Item>> {
        self.read_next(cx)
    }
}
