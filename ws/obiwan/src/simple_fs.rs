//! This module is a simple abstraction over the filesystem to the
//! degree that the TFTP protocol will need. It's main purpose is to
//! facilitate unit testing.

use std::{fmt::Debug, io::SeekFrom, path::Path, sync::Arc};

use async_trait::async_trait;
use tokio::{
    io::{AsyncReadExt, AsyncSeekExt},
    sync::Mutex,
};

#[async_trait]
pub trait File: Debug + Send + Sync + Sized + Clone {
    type Error: std::error::Error + Send + Sync + 'static;

    /// Reads as many bytes as possible into `buf`. Returns the number
    /// of bytes read. If less bytes are read than `buf` has space, the
    /// file has ended.
    async fn read(&self, offset: u64, buf: &mut [u8]) -> Result<usize, Self::Error>;
}

#[async_trait]
pub trait Filesystem: Debug + Send + Sync + Clone {
    type File: File;
    type Error: std::error::Error + Send + Sync + 'static;

    /// Open a file for reading.
    async fn open(&self, path: &Path) -> Result<Self::File, Self::Error>;
}

#[derive(Debug, Clone)]
pub struct AsyncFile {
    file: Arc<Mutex<tokio::fs::File>>,
}

impl From<tokio::fs::File> for AsyncFile {
    fn from(file: tokio::fs::File) -> Self {
        Self {
            file: Arc::new(Mutex::new(file)),
        }
    }
}

#[async_trait]
impl File for AsyncFile {
    type Error = std::io::Error;

    async fn read(&self, offset: u64, buf: &mut [u8]) -> Result<usize, Self::Error> {
        let mut file = self.file.lock().await;

        file.seek(SeekFrom::Start(offset)).await?;

        let mut offset = 0;

        loop {
            let bytes_read = file.read(&mut buf[offset..]).await?;
            offset += bytes_read;

            if bytes_read == 0 || offset == buf.len() {
                break;
            }
        }

        Ok(offset)
    }
}

#[derive(Debug, Clone, Default)]
pub struct AsyncFilesystem {}

#[async_trait]
impl Filesystem for AsyncFilesystem {
    type File = AsyncFile;
    type Error = std::io::Error;

    async fn open(&self, path: &Path) -> Result<Self::File, Self::Error> {
        tokio::fs::File::open(path).await.map(AsyncFile::from)
    }
}

#[cfg(test)]
#[async_trait]
impl File for Vec<u8> {
    type Error = std::io::Error;

    async fn read(&self, offset: u64, buf: &mut [u8]) -> Result<usize, Self::Error> {
        use std::io::ErrorKind;

        if offset
            >= u64::try_from(self.len())
                .map_err(|_| ())
                .map_err(|_| Self::Error::new(ErrorKind::Other, "Conversion error"))?
        {
            return Ok(0);
        }

        let offset = usize::try_from(offset)
            .map_err(|_| Self::Error::new(ErrorKind::Other, "Conversion error"))?;
        let len = buf.len().min(self.len() - offset);

        buf[..len].copy_from_slice(&self[offset..(offset + len)]);
        Ok(len)
    }
}

#[cfg(test)]
pub type MapFilesystem = std::collections::BTreeMap<std::path::PathBuf, Vec<u8>>;

#[cfg(test)]
#[async_trait]
impl Filesystem for MapFilesystem {
    type File = Vec<u8>;
    type Error = std::io::Error;

    async fn open(&self, path: &Path) -> Result<Self::File, Self::Error> {
        self.get(path)
            .ok_or(std::io::Error::from_raw_os_error(22))
            .cloned()
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

        let file = map
            .open(Path::new("/foo"))
            .await
            .expect("Failed to open file");

        let mut buf = [0; 64];

        assert_eq!(file.read(300, &mut buf).await.unwrap(), 0); // EOF

        assert_eq!(file.read(0, &mut buf).await.unwrap(), 4);
        assert_eq!(&buf[0..4], &[1, 2, 3, 4]);

        assert_eq!(file.read(3, &mut buf).await.unwrap(), 1);
        assert_eq!(&buf[0..1], &[4]);
    }
}
