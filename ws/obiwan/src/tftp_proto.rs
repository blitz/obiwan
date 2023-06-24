//! This module implements the TFTP protocol in terms of [`simple_proto`].

use std::{
    path::{Path, PathBuf},
    time::Duration,
};

use crate::{
    path::normalize,
    simple_fs::{self, File},
    simple_proto::{self, ConnectionStatus, Event, Response},
    tftp,
};

use anyhow::{anyhow, Result};
use async_trait::async_trait;
use log::{debug, warn};

const DEFAULT_TFTP_TIMEOUT: Duration = Duration::from_secs(1);
const DEFAULT_TFTP_BLKSIZE: usize = 512;

/// How many times do we resend packets, if we don't get a response.
const MAX_RETRANSMISSIONS: u32 = 5;

/// The current state of the TFTP connection.
#[derive(Debug)]
pub enum Connection<FS: simple_fs::Filesystem> {
    Dead,
    WaitingForInitialPacket {
        filesystem: FS,
        root: PathBuf,
    },
    ReadingFile {
        file: FS::File,

        /// The last block we acked. Note that this is not `u16` as
        /// the block number in TFTP packets, because otherwise we
        /// would be limited to small packet sizes.
        last_acked_block: u64,

        /// How many timeout events have we received for the current block.
        timeout_events: u32,

        /// We are waiting for the last ACK.
        last_was_final: bool,
    },
}

fn error_response(error_code: u16, error_msg: &str) -> Response<tftp::Packet> {
    warn!("Sending error to client: {error_code} {error_msg}");

    Response {
        packet: Some(tftp::Packet::Error {
            error_code,
            error_msg: error_msg.to_owned(),
        }),
        next_status: ConnectionStatus::Terminated,
    }
}

fn no_response() -> Response<tftp::Packet> {
    Response {
        packet: None,
        next_status: ConnectionStatus::Terminated,
    }
}

impl<FS: simple_fs::Filesystem> Connection<FS> {
    pub fn new_with_filesystem(filesystem: FS, root: impl AsRef<Path>) -> Self {
        Self::WaitingForInitialPacket {
            filesystem,
            root: root.as_ref().to_path_buf(),
        }
    }

    async fn read_block(file: &mut FS::File, block: u64) -> Result<Vec<u8>> {
        assert!(block >= 1);

        let mut buf = [0_u8; DEFAULT_TFTP_BLKSIZE];
        let size = file
            .read((block - 1) * u64::try_from(DEFAULT_TFTP_BLKSIZE)?, &mut buf)
            .await?;

        Ok(buf[0..size].to_vec())
    }

    async fn send_block(
        mut file: FS::File,
        block: u64,
        timeouts: u32,
    ) -> Result<(Self, Response<tftp::Packet>)> {
        assert!(block > 0);

        let data = Self::read_block(&mut file, block).await?;
        assert!(data.len() <= DEFAULT_TFTP_BLKSIZE);

        Ok((
            Self::ReadingFile {
                file,
                last_acked_block: block - 1,
                timeout_events: timeouts,
                last_was_final: data.len() < DEFAULT_TFTP_BLKSIZE,
            },
            Response {
                packet: Some(tftp::Packet::Data {
                    block: u16::try_from(block & 0xffff).unwrap(),
                    data,
                }),
                next_status: ConnectionStatus::WaitingForPacket(DEFAULT_TFTP_TIMEOUT),
            },
        ))
    }

    async fn handle_initial_read(
        filesystem: FS,
        root: &Path,
        path: &Path,
    ) -> Result<(Self, Response<tftp::Packet>)> {
        match filesystem
            .open(
                &root.join(
                    normalize(path)
                        .ok_or_else(|| anyhow!("Failed to normalize path: {}", path.display()))?,
                ),
            )
            .await
        {
            Ok(file) => Self::send_block(file, 1, 0).await,
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
        filesystem: FS,
        root: &Path,
        event: Event<tftp::Packet>,
    ) -> Result<(Self, Response<tftp::Packet>)> {
        match event {
            Event::PacketReceived(p) => match p {
                tftp::Packet::Rrq {
                    filename,
                    mode: _,
                    options: _,
                } => Self::handle_initial_read(filesystem, root, &filename).await,
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
        file: FS::File,
        mut last_acked_block: u64,
        mut timeouts: u32,
        last_was_final: bool,
        event: Event<tftp::Packet>,
    ) -> Result<(Self, Response<tftp::Packet>)> {
        match event {
            Event::PacketReceived(packet) => match packet {
                tftp::Packet::Ack { block } => {
                    if u64::from(block) == (last_acked_block + 1) & 0xffff {
                        last_acked_block += 1;

                        if last_was_final {
                            debug!("Successfully sent {last_acked_block} blocks.");
                            return Ok((Self::Dead, no_response()));
                        }
                    } else {
                        return Ok((
                            Self::Dead,
                            error_response(0, "Unexpected ACK for block {block}"),
                        ));
                    }
                }
                tftp::Packet::Error {
                    error_code,
                    error_msg,
                } => {
                    warn!("Client sent error: {error_code} {error_msg}");
                    return Ok((Self::Dead, no_response()));
                }
                _ => {
                    return Ok((
                        Self::Dead,
                        error_response(0, "Received unexpected packet. Closing connection."),
                    ));
                }
            },
            Event::Timeout => {
                timeouts += 1;

                if timeouts > MAX_RETRANSMISSIONS {
                    warn!("Client timed out sending ACKs.");
                    return Ok((Self::Dead, no_response()));
                }
            }
        }

        Self::send_block(file, last_acked_block + 1, timeouts).await
    }
}

impl Connection<simple_fs::AsyncFilesystem> {
    pub fn new(root: impl AsRef<Path>) -> Self {
        Self::new_with_filesystem(simple_fs::AsyncFilesystem::default(), root)
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
            Self::WaitingForInitialPacket { filesystem, root } => {
                Self::handle_initial_event(filesystem.clone(), root, event).await?
            }
            Self::ReadingFile {
                file,
                last_acked_block,
                timeout_events,
                last_was_final,
            } => {
                Self::handle_reading_file_event(
                    file.clone(),
                    *last_acked_block,
                    *timeout_events,
                    *last_was_final,
                    event,
                )
                .await?
            }
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
        let mut con = Connection::new_with_filesystem(fs, "/");

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
