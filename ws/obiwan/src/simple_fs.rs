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

#[cfg(test)]
#[async_trait]
impl File for Vec<u8> {
    type Error = ();

    async fn read(&mut self, offset: u64, buf: &mut [u8]) -> Result<usize, Self::Error> {
        if offset >= u64::try_from(self.len()).map_err(|_| ())? {
            return Ok(0);
        }

        let offset = usize::try_from(offset).map_err(|_| ())?;
        let len = buf.len().min(self.len() - offset);

        buf[..len].copy_from_slice(&self[offset..(offset + len)]);
        Ok(len)
    }
}

#[cfg(test)]
#[async_trait]
impl Filesystem for std::collections::BTreeMap<std::path::PathBuf, Vec<u8>> {
    type File = Vec<u8>;
    type Error = ();

    async fn open(&self, path: &Path) -> Result<Self::File, Self::Error> {
        self.get(path).ok_or(()).cloned()
    }
}

#[cfg(test)]
mod tests {
    use std::{collections::BTreeMap, path::PathBuf, str::FromStr};

    use super::*;

    #[tokio::test]
    async fn can_read_btree_fs() {
        let map: BTreeMap<std::path::PathBuf, Vec<u8>> =
            BTreeMap::from([(PathBuf::from_str("/foo").unwrap(), vec![1, 2, 3, 4])]);

        let mut file = map
            .open(Path::new("/foo"))
            .await
            .expect("Failed to open file");

        let mut buf = [0; 64];

        assert_eq!(file.read(300, &mut buf).await, Ok(0)); // EOF

        assert_eq!(file.read(0, &mut buf).await, Ok(4));
        assert_eq!(&buf[0..4], &[1, 2, 3, 4]);

        assert_eq!(file.read(3, &mut buf).await, Ok(1));
        assert_eq!(&buf[0..1], &[4]);
    }
}
