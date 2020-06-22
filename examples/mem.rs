use async_trait::async_trait;
use nbd_async::BlockDevice;

struct MemDev {
    data: Vec<u8>,
    num_blocks: usize,
    block_size: u32,
}
impl MemDev {
    fn new(block_size: u32, num_blocks: usize) -> Self {
        Self {
            data: vec![0; num_blocks * block_size as usize],
            num_blocks,
            block_size,
        }
    }
}

#[async_trait]
impl BlockDevice for MemDev {
    async fn read(&mut self, offset: u64, buf: &mut [u8]) -> Result<(), std::io::Error> {
        let offset = offset as usize;
        buf.copy_from_slice(&self.data[offset..offset + buf.len()]);
        Ok(())
    }
    async fn write(&mut self, offset: u64, buf: &[u8]) -> Result<(), std::io::Error> {
        let offset = offset as usize;
        self.data[offset..offset + buf.len()].copy_from_slice(buf);
        Ok(())
    }
    fn block_size(&self) -> u32 {
        self.block_size
    }
    fn block_count(&self) -> u64 {
        self.num_blocks as u64
    }
}

#[tokio::main]
async fn main() {
    let nbd_path = std::env::args().nth(1).expect("NDB device path");
    let dev = MemDev::new(512, 128);
    nbd_async::attach_device(nbd_path, dev).await.unwrap();
}
