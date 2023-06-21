//! This module implements the TFTP protocol in terms of [`simple_proto`].

use std::{path::Path, time::Duration};

use crate::{
    simple_fs::{self, File},
    simple_proto::{self, ConnectionStatus, Event, Response},
    tftp,
};

use anyhow::Result;
use async_trait::async_trait;

const DEFAULT_TFTP_TIMEOUT: Duration = Duration::from_secs(1);
const DEFAULT_TFTP_BLKSIZE: usize = 512;

/// The current state of the TFTP connection.
#[derive(Debug)]
pub enum Connection<FS: simple_fs::Filesystem> {
    Dead,
    WaitingForInitialPacket {
        filesystem: FS,
    },
    ReadingFile {
        file: FS::File,

        /// The last block we acked. Note that this is not `u16` as
        /// the block number in TFTP packets, because otherwise we
        /// would be limited to small packet sizes.
        last_acked_block: u64,
    },
}

fn error_response(error_code: u16, error_msg: &str) -> Response<tftp::Packet> {
    Response {
        packet: Some(tftp::Packet::Error {
            error_code,
            error_msg: error_msg.to_owned(),
        }),
        next_status: ConnectionStatus::Terminated,
    }
}

impl<FS: simple_fs::Filesystem> Connection<FS> {
    pub fn new_with_filesystem(filesystem: FS) -> Self {
        Self::WaitingForInitialPacket { filesystem }
    }

    async fn read_block(file: &mut FS::File, block: u64) -> Result<Vec<u8>> {
        let mut buf = [0_u8; DEFAULT_TFTP_BLKSIZE];
        let size = file
            .read((block - 1) * u64::try_from(DEFAULT_TFTP_BLKSIZE)?, &mut buf)
            .await?;

        Ok(buf[0..size].to_vec())
    }

    async fn handle_initial_read(
        filesystem: &mut FS,
        path: &Path,
    ) -> Result<(Self, Response<tftp::Packet>)> {
        match filesystem.open(path).await {
            Ok(mut file) => {
                let data = Self::read_block(&mut file, 1).await?;
                Ok((
                    Self::ReadingFile {
                        file,
                        last_acked_block: 0,
                    },
                    Response {
                        packet: Some(tftp::Packet::Data { block: 1, data }),
                        next_status: ConnectionStatus::WaitingForPacket(DEFAULT_TFTP_TIMEOUT),
                    },
                ))
            }
            Err(err) => Ok((
                Self::Dead,
                error_response(
                    0, /* TODO File not found */
                    &format!("Failed to open file: {err}"),
                ),
            )),
        }
    }

    async fn handle_initial_event(
        filesystem: &mut FS,
        event: Event<tftp::Packet>,
    ) -> Result<(Self, Response<tftp::Packet>)> {
        match event {
            Event::PacketReceived(p) => match p {
                tftp::Packet::Rrq {
                    filename,
                    mode: _,
                    options: _,
                } => Self::handle_initial_read(filesystem, &filename).await,
                tftp::Packet::Wrq { .. } => Ok((
                    Self::Dead,
                    error_response(
                        2, /* Access violation */
                        "This server only supports reading files",
                    ),
                )),
                _ => Ok((
                    Self::Dead,
                    error_response(0 /* TODO */, "Initial request is not Rrq or Wrq"),
                )),
            },
            Event::Timeout => panic!("Can't receive timeout as initial event"),
        }
    }

    async fn handle_reading_file_event(
        _file: &mut FS::File,
        _last_acked_block: u64,
    ) -> Result<(Self, Response<tftp::Packet>)> {
        todo!()
    }
}

impl Connection<simple_fs::AsyncFilesystem> {
    pub fn new() -> Self {
        Self::new_with_filesystem(simple_fs::AsyncFilesystem::default())
    }
}

#[async_trait]
impl<FS: simple_fs::Filesystem> simple_proto::SimpleUdpProtocol for Connection<FS> {
    type Packet = tftp::Packet;
    type Error = anyhow::Error;

    async fn handle_event(
        &mut self,
        event: Event<Self::Packet>,
    ) -> Result<simple_proto::Response<Self::Packet>, Self::Error> {
        let (new_self, response) = match self {
            Self::Dead => panic!(
                "Should not receive events on a dead connection: {:?}",
                event
            ),
            Self::WaitingForInitialPacket { filesystem } => {
                Self::handle_initial_event(filesystem, event).await?
            }
            Self::ReadingFile {
                file,
                last_acked_block,
            } => Self::handle_reading_file_event(file, *last_acked_block).await?,
        };

        *self = new_self;
        Ok(response)
    }
}

#[cfg(test)]
mod tests {
    use std::{path::PathBuf, str::FromStr};

    use crate::simple_proto::{ConnectionStatus, SimpleUdpProtocol};

    use super::*;

    #[tokio::test]
    async fn simple_read() {
        let mut file_contents = [0xab_u8; 513].to_vec();

        // Make the contents more interesting.
        file_contents[2] = 0x12;
        file_contents[512] = 0x23;

        let fs = simple_fs::MapFilesystem::from([(
            PathBuf::from_str("/foo").unwrap(),
            file_contents.clone(),
        )]);
        let mut con = Connection::new_with_filesystem(fs);

        assert_eq!(
            con.handle_event(Event::PacketReceived(tftp::Packet::Rrq {
                filename: PathBuf::from("/foo"),
                mode: tftp::RequestMode::Octet,
                options: vec![]
            }))
            .await
            .unwrap(),
            Response {
                packet: Some(tftp::Packet::Data {
                    block: 1,
                    data: file_contents[0..512].to_vec()
                }),
                next_status: ConnectionStatus::WaitingForPacket(DEFAULT_TFTP_TIMEOUT)
            }
        );

        assert_eq!(
            con.handle_event(Event::PacketReceived(tftp::Packet::Ack { block: 1 }))
                .await
                .unwrap(),
            Response {
                packet: Some(tftp::Packet::Data {
                    block: 2,
                    data: file_contents[512..].to_vec()
                }),
                next_status: ConnectionStatus::WaitingForPacket(DEFAULT_TFTP_TIMEOUT)
            }
        );
    }
}
