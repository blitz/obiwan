//! This module is a simple abstraction over the filesystem to the
//! degree that the TFTP protocol will need. It's main purpose is to
//! facilitate unit testing.

use std::{io::SeekFrom, path::Path};

use async_trait::async_trait;
use tokio::io::{AsyncReadExt, AsyncSeekExt};

#[async_trait]
pub trait File {
    type Error;

    /// Reads as many bytes as possible into `buf`. Returns the number
    /// of bytes read. If less bytes are read than `buf` has space, the
    /// file has ended.
    async fn read(&mut self, offset: u64, buf: &mut [u8]) -> Result<usize, Self::Error>;
}

#[async_trait]
trait Filesystem {
    type File: File;
    type Error;

    /// Open a file for reading.
    async fn open(&self, path: &Path) -> Result<Self::File, Self::Error>;
}

#[async_trait]
impl File for tokio::fs::File {
    type Error = std::io::Error;

    async fn read(&mut self, offset: u64, buf: &mut [u8]) -> Result<usize, Self::Error> {
        self.seek(SeekFrom::Start(offset)).await?;

        let mut offset = 0;

        loop {
            let bytes_read = AsyncReadExt::read(self, &mut buf[offset..]).await?;
            offset += bytes_read;

            if bytes_read == 0 || offset == buf.len() {
                break;
            }
        }

        Ok(offset)
    }
}

#[derive(Debug, Clone)]
struct AsyncFilesystem {}

#[async_trait]
impl Filesystem for AsyncFilesystem {
    type File = tokio::fs::File;
    type Error = std::io::Error;

    async fn open(&self, path: &Path) -> Result<Self::File, Self::Error> {
        tokio::fs::File::open(path).await
    }
}
