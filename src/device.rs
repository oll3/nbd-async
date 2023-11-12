use async_trait::async_trait;
use core::pin::Pin;
use core::task::{Context, Poll};
use futures_util::stream::{Stream, StreamExt};
use std::io;
use std::os::unix::io::AsRawFd;
use std::path::Path;
use std::thread::JoinHandle;
use tokio::{
    fs::OpenOptions, io::AsyncRead, io::AsyncReadExt, io::AsyncWrite, io::AsyncWriteExt,
    io::ReadBuf, net::UnixStream,
};

use crate::nbd::Request;
use crate::{nbd, sys};

/// possible control message to device
#[derive(Debug, Copy, Clone)]
pub enum Control<T> {
    Notify(T),
    Shutdown,
}

/// use as a control stream if your nbd device does not require
/// control
pub struct NoControl;

impl Stream for NoControl {
    type Item = Control<()>;
    fn poll_next(self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        Poll::Pending
    }
}

/// A block device.
#[async_trait(?Send)]
pub trait BlockDevice<N = ()> {
    /// Read a block from offset.
    async fn read(&mut self, offset: u64, buf: &mut [u8]) -> io::Result<()>;
    /// Write a block of data at offset.
    async fn write(&mut self, _offset: u64, _buf: &[u8]) -> io::Result<()> {
        Err(io::ErrorKind::InvalidInput.into())
    }
    /// Flushes write buffers to the underlying storage medium
    async fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
    /// Marks blocks as unused
    async fn trim(&mut self, _offset: u64, _size: usize) -> io::Result<()> {
        Ok(())
    }

    /// called if a new control message is available on control stream
    async fn control(&mut self, _control: &Control<N>) -> io::Result<()> {
        Ok(())
    }
}

pub struct Server {
    do_it_thread: Option<JoinHandle<io::Result<()>>>,
    file: tokio::fs::File,
}

impl Drop for Server {
    fn drop(&mut self) {
        let _ = sys::disconnect(&self.file);
        if let Some(do_it_thread) = self.do_it_thread.take() {
            do_it_thread.join().expect("join thread").unwrap();
        }
    }
}

/// Attach a socket to a NBD device file.
pub async fn attach_device<P, S>(
    path: P,
    socket: S,
    block_size: u32,
    block_count: u64,
    read_only: bool,
) -> io::Result<Server>
where
    P: AsRef<Path>,
    S: AsRawFd + Send + 'static,
{
    let file = OpenOptions::new()
        .read(true)
        .write(true)
        .open(path.as_ref())
        .await?;

    sys::set_block_size(&file, block_size)?;
    sys::set_size_blocks(&file, block_count)?;
    sys::set_timeout(&file, 10)?;
    sys::clear_sock(&file)?;

    let inner_file = file.try_clone().await?;
    let do_it_thread = Some(std::thread::spawn(move || -> io::Result<()> {
        sys::set_sock(&inner_file, socket.as_raw_fd())?;
        if read_only {
            sys::set_flags(&inner_file, sys::HAS_FLAGS | sys::READ_ONLY)?;
        } else {
            sys::set_flags(&inner_file, 0)?;
        }
        // The do_it ioctl will block until device is disconnected, hence
        // the separate thread.
        sys::do_it(&inner_file)?;
        let _ = sys::clear_sock(&inner_file);
        let _ = sys::clear_queue(&inner_file);
        Ok(())
    }));
    Ok(Server { do_it_thread, file })
}

/// Serve a local block device through a NBD dev file.
pub async fn serve_local_nbd<P, B, CT, N>(
    path: P,
    block_size: u32,
    block_count: u64,
    read_only: bool,
    block_device: B,
    control: CT,
) -> io::Result<B>
where
    P: AsRef<Path>,
    B: Unpin + BlockDevice<N>,
    CT: Stream<Item = Control<N>> + Unpin,
{
    let (sock, kernel_sock) = UnixStream::pair()?;
    let _server = attach_device(path, kernel_sock, block_size, block_count, read_only).await?;
    serve_nbd(block_device, sock, control).await
}

struct RequestStream<C> {
    client: Option<C>,
    read_buf: [u8; nbd::SIZE_OF_REQUEST],
}

/// Serve a block device using a read/write client.
/// the wake_up duration defines how often the wake_up method of the block device
/// is called when it's not handling any actual nbd requests.
/// in other words, if the device is ideal for this wake_up duration the wake_up method
/// is called
pub async fn serve_nbd<B, C, CT, N>(
    mut block_device: B,
    client: C,
    mut control: CT,
) -> io::Result<B>
where
    B: Unpin + BlockDevice<N>,
    C: AsyncRead + AsyncWrite + Unpin,
    CT: Stream<Item = Control<N>> + Unpin,
{
    let mut stream = RequestStream {
        client: Some(client),
        read_buf: [0; nbd::SIZE_OF_REQUEST],
    };

    let mut reply_buf = vec![];
    let mut write_buf = vec![];
    loop {
        tokio::select! {
            result = stream.next() => {
                let result = match result {
                    Some(result) => result,
                    None => break,
                };

                let request = result?;
                let request_handler = stream.client.as_mut().unwrap();

                serve_nbd_request(
                    &mut block_device,
                    request_handler,
                    request,
                    &mut reply_buf,
                    &mut write_buf,
                )
                .await?;
            },
            control = control.next() => {
                let msg = control.unwrap_or(Control::Shutdown);
                block_device.control(&msg).await?;
                if let Control::Shutdown = msg {
                    return Ok(block_device)
                }
            }
        }
    }
    Ok(block_device)
}

async fn serve_nbd_request<B, C, N>(
    block_device: &mut B,
    request_handler: &mut C,
    request: Request,
    reply_buf: &mut Vec<u8>,
    write_buf: &mut Vec<u8>,
) -> io::Result<()>
where
    B: Unpin + BlockDevice<N>,
    C: AsyncRead + AsyncWrite + Unpin,
{
    let mut reply = nbd::Reply::from_request(&request);
    match request.command {
        nbd::Command::Read => {
            reply_buf.resize(nbd::SIZE_OF_REPLY + request.len, 0);
            if let Err(err) = block_device
                .read(request.from, &mut reply_buf[nbd::SIZE_OF_REPLY..])
                .await
            {
                // On error we shall reply with error code but no payload.
                reply.error = err.raw_os_error().unwrap_or(nix::errno::Errno::EIO as i32);
                reply_buf.resize(nbd::SIZE_OF_REPLY, 0);
            }
            reply.write_to_slice(&mut reply_buf[..])?;
        }
        nbd::Command::Write => {
            write_buf.resize(request.len, 0);
            request_handler.read_exact(write_buf).await?;
            if let Err(err) = block_device.write(request.from, &write_buf[..]).await {
                reply.error = err.raw_os_error().unwrap_or(nix::errno::Errno::EIO as i32);
            }
            reply.append_to_vec(reply_buf)?;
        }
        nbd::Command::Flush => {
            if let Err(err) = block_device.flush().await {
                reply.error = err.raw_os_error().unwrap_or(nix::errno::Errno::EIO as i32);
            }
            reply.append_to_vec(reply_buf)?;
        }
        nbd::Command::Trim => {
            if let Err(err) = block_device.trim(request.from, request.len).await {
                reply.error = err.raw_os_error().unwrap_or(nix::errno::Errno::EIO as i32);
            }
            reply.append_to_vec(reply_buf)?;
        }
        nbd::Command::Disc => unimplemented!(),
        nbd::Command::WriteZeroes => unimplemented!(),
    }
    request_handler.write_all(reply_buf).await?;
    reply_buf.clear();

    Ok(())
}

impl<C> RequestStream<C>
where
    C: AsyncRead + AsyncWrite + Unpin,
{
    fn read_next(&mut self, cx: &mut Context) -> Poll<Option<io::Result<nbd::Request>>> {
        let client = match self.client {
            Some(ref mut client) => client,
            None => return Poll::Ready(None),
        };
        let mut read_buf = ReadBuf::new(&mut self.read_buf);
        let rc = Pin::new(client).poll_read(cx, &mut read_buf);
        match rc {
            Poll::Ready(Ok(())) => {
                if read_buf.filled().is_empty() {
                    return Poll::Ready(None);
                }
                if read_buf.filled().len() != nbd::SIZE_OF_REQUEST {
                    return Poll::Ready(Some(Err(io::Error::from(io::ErrorKind::UnexpectedEof))));
                }
            }
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

impl<C> Stream for RequestStream<C>
where
    C: AsyncRead + AsyncWrite + Unpin,
{
    type Item = io::Result<nbd::Request>;
    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context) -> Poll<Option<Self::Item>> {
        self.read_next(cx)
    }
}
