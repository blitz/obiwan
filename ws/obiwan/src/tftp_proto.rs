//! This module implements the TFTP protocol in terms of [`simple_proto`].

use std::{
    path::{Path, PathBuf},
    time::Duration,
};

use crate::{
    path::normalize,
    simple_fs::{self, File},
    simple_proto::{self, ConnectionStatus, Event, Response},
    tftp::{self, RequestOption},
};

use anyhow::{anyhow, Result};
use async_trait::async_trait;
use log::{debug, info, warn};

const DEFAULT_TFTP_TIMEOUT: Duration = Duration::from_secs(1);
const DEFAULT_TFTP_BLKSIZE: u16 = 512;

/// How many times do we resend packets, if we don't get a response.
const MAX_RETRANSMISSIONS: u32 = 5;

/// The options sent by the client that we acknowledged.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
struct AcceptedOptions {
    block_size: Option<u16>,
}

impl AcceptedOptions {
    fn to_option_vec(self) -> Vec<RequestOption> {
        let mut res = vec![];

        if let Some(block_size) = self.block_size {
            res.push(RequestOption {
                name: "blksize".to_string(),
                value: block_size.to_string(),
            })
        }

        res
    }
}

/// The current state of the TFTP connection.
#[derive(Debug)]
pub enum Connection<FS: simple_fs::Filesystem> {
    /// The connection is terminated. No further packets are expected.
    Dead,
    /// We haven't seen an initial packet yet.
    WaitingForInitialPacket { filesystem: FS, root: PathBuf },

    /// We have sent an OACK packet and wait for the corresponding ACK with block 0.
    AcknowledgingOptions {
        file: FS::File,

        /// How many timeout events have we received for this packet.
        timeout_events: u32,

        /// The list of options that we want to acknowledge.
        acknowledged_options: Vec<RequestOption>,

        /// The block size for data packets.
        block_size: u16,
    },

    /// The client successfully requested a file and we have managed
    /// to open it. Now we are reading the contents.
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

        /// The block size for data packets. This is negotiated via options when the connection is established.
        block_size: u16,
    },
}

impl<FS: simple_fs::Filesystem> Connection<FS> {
    pub fn new_with_filesystem(filesystem: FS, root: impl AsRef<Path>) -> Self {
        Self::WaitingForInitialPacket {
            filesystem,
            root: root.as_ref().to_path_buf(),
        }
    }

    async fn read_block(file: &mut FS::File, block: u64, block_size: u16) -> Result<Vec<u8>> {
        assert!(block >= 1);

        let mut buf = Vec::new();
        buf.resize(usize::from(block_size), 0);

        let size = file
            .read((block - 1) * u64::try_from(block_size)?, &mut buf)
            .await?;

        Ok(buf[0..size].to_vec())
    }

    async fn ignore_packet(
        file: FS::File,
        block: u64,
        timeouts: u32,
        last_was_final: bool,
        block_size: u16,
    ) -> Result<(Self, Response<tftp::Packet>)> {
        Ok((
            Self::ReadingFile {
                file,
                last_acked_block: block,
                timeout_events: timeouts,
                last_was_final,
                block_size,
            },
            Response {
                packet: None,
                next_status: ConnectionStatus::WaitingForPacket(DEFAULT_TFTP_TIMEOUT),
            },
        ))
    }

    /// Drop the connection without sending an error.
    fn drop_connection() -> Result<(Self, Response<tftp::Packet>)> {
        Ok((
            Self::Dead,
            Response {
                packet: None,
                next_status: ConnectionStatus::Terminated,
            },
        ))
    }

    fn drop_connection_with_error<S: Into<String>>(
        error_code: u16,
        error_msg: S,
    ) -> Result<(Self, Response<tftp::Packet>)> {
        let error_msg: String = error_msg.into();

        warn!("Sending error to client: {error_code} {error_msg}");

        Ok((
            Self::Dead,
            Response {
                packet: Some(tftp::Packet::Error {
                    error_code,
                    error_msg,
                }),
                next_status: ConnectionStatus::Terminated,
            },
        ))
    }

    async fn send_block(
        mut file: FS::File,
        block: u64,
        timeouts: u32,
        block_size: u16,
    ) -> Result<(Self, Response<tftp::Packet>)> {
        assert!(block > 0);
        assert!(block_size > 0);

        let data = Self::read_block(&mut file, block, block_size).await?;
        assert!(data.len() <= usize::from(block_size));

        Ok((
            Self::ReadingFile {
                file,
                last_acked_block: block - 1,
                timeout_events: timeouts,
                last_was_final: data.len() < usize::from(block_size),
                block_size,
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

    async fn acknowledge_options(
        file: FS::File,
        acknowledged_options: Vec<RequestOption>,
        timeout_events: u32,
        block_size: u16,
    ) -> Result<(Self, Response<tftp::Packet>)> {
        Ok((
            Self::AcknowledgingOptions {
                file,
                acknowledged_options: acknowledged_options.clone(),
                block_size,
                timeout_events,
            },
            Response {
                packet: Some(tftp::Packet::OAck {
                    options: acknowledged_options,
                }),
                next_status: ConnectionStatus::WaitingForPacket(DEFAULT_TFTP_TIMEOUT),
            },
        ))
    }

    async fn handle_initial_read(
        filesystem: FS,
        root: &Path,
        path: &Path,
        accepted_options: AcceptedOptions,
    ) -> Result<(Self, Response<tftp::Packet>)> {
        let local_path = root.join(
            normalize(path)
                .ok_or_else(|| anyhow!("Failed to normalize path: {}", path.display()))?,
        );

        info!("TFTP READ {} -> {}", path.display(), local_path.display());

        match filesystem.open(&local_path).await {
            Ok(file) => {
                let block_size = accepted_options.block_size.unwrap_or(DEFAULT_TFTP_BLKSIZE);
                let option_vec = accepted_options.to_option_vec();

                debug!("Accepted these options: {option_vec:?}");

                if option_vec.is_empty() {
                    Self::send_block(file, 1, 0, block_size).await
                } else {
                    Self::acknowledge_options(file, option_vec, 0, block_size).await
                }
            }
            Err(err) => Self::drop_connection_with_error(
                tftp::error::UNDEFINED,
                format!("Failed to open file {}: {err}", local_path.display()),
            ),
        }
    }

    /// Take the client's proposed options and see what is useful for us.
    fn accept_options(options: &[RequestOption]) -> AcceptedOptions {
        let mut block_size: Option<u16> = None;

        for option in options {
            if option.name.eq_ignore_ascii_case("blksize") {
                match option.value.parse::<u16>() {
                    Ok(parsed_block_size) if (8..=65464).contains(&parsed_block_size) => {
                        block_size = Some(parsed_block_size);
                    }
                    _ => {
                        warn!("Ignoring invalid block size: {}", option.value);
                    }
                }
            } else {
                debug!("Ignoring unknown option {}={}", option.name, option.value);
            }
        }

        AcceptedOptions { block_size }
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
                    options,
                } => {
                    Self::handle_initial_read(
                        filesystem,
                        root,
                        &filename,
                        Self::accept_options(&options),
                    )
                    .await
                }
                tftp::Packet::Wrq { .. } => Self::drop_connection_with_error(
                    tftp::error::ACCESS_VIOLATION,
                    "This server only supports reading files",
                ),
                _ => Self::drop_connection_with_error(
                    tftp::error::ILLEGAL_OPERATION,
                    "Initial request is not Rrq or Wrq",
                ),
            },
            Event::Timeout => panic!("Can't receive timeout as initial event"),
        }
    }

    async fn handle_option_acknowledgement(
        file: FS::File,
        timeout_events: u32,
        acknowledged_options: Vec<RequestOption>,
        block_size: u16,
        event: Event<tftp::Packet>,
    ) -> Result<(Self, Response<tftp::Packet>)> {
        match event {
            Event::PacketReceived(p) => match p {
                tftp::Packet::Ack { block } if block == 0 => {
                    Self::send_block(file, 1, 0, block_size).await
                }
                _ => Self::drop_connection_with_error(
                    tftp::error::ILLEGAL_OPERATION,
                    "Expected ACK 0 as OACK response",
                ),
            },
            Event::Timeout => {
                if timeout_events >= MAX_RETRANSMISSIONS {
                    warn!("Client timed out sending first ACK.");
                    Self::drop_connection()
                } else {
                    debug!("Timeout waiting for ACK for options, resending...",);

                    Self::acknowledge_options(
                        file,
                        acknowledged_options,
                        timeout_events,
                        block_size,
                    )
                    .await
                }
            }
        }
    }

    async fn handle_reading_file_event(
        file: FS::File,
        mut last_acked_block: u64,
        mut timeouts: u32,
        last_was_final: bool,
        block_size: u16,
        event: Event<tftp::Packet>,
    ) -> Result<(Self, Response<tftp::Packet>)> {
        match event {
            Event::PacketReceived(packet) => match packet {
                tftp::Packet::Ack { block } => {
                    let expected_block = last_acked_block + 1;

                    debug!("Client acknowledged block {block:#x}, we expect {expected_block:#x}.");

                    if u64::from(block) == expected_block & 0xffff {
                        timeouts = 0;
                        last_acked_block += 1;

                        if last_was_final {
                            debug!("Successfully sent {last_acked_block} blocks.");
                            return Self::drop_connection();
                        }
                    } else {
                        debug!("Unexpected ACK. Ignoring.");
                        return Self::ignore_packet(
                            file,
                            last_acked_block,
                            timeouts,
                            last_was_final,
                            block_size,
                        )
                        .await;
                    }
                }
                tftp::Packet::Error {
                    error_code,
                    error_msg,
                } => {
                    warn!("Client sent error: {error_code} {error_msg}");
                    return Self::drop_connection();
                }
                _ => {
                    return Self::drop_connection_with_error(
                        tftp::error::ILLEGAL_OPERATION,
                        "Received unexpected packet. Closing connection.",
                    );
                }
            },
            Event::Timeout => {
                timeouts += 1;

                if timeouts > MAX_RETRANSMISSIONS {
                    warn!("Client timed out sending ACKs.");
                    return Self::drop_connection();
                } else {
                    debug!(
                        "Timeout waiting for ACK for block {:x}, resending...",
                        last_acked_block + 1
                    );
                }
            }
        }

        debug!("Sending block {:x}.", last_acked_block + 1);
        Self::send_block(file, last_acked_block + 1, timeouts, block_size).await
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
            Self::AcknowledgingOptions {
                file,
                timeout_events,
                acknowledged_options,
                block_size,
            } => {
                Self::handle_option_acknowledgement(
                    file.clone(),
                    *timeout_events,
                    acknowledged_options.clone(),
                    *block_size,
                    event,
                )
                .await?
            }
            Self::ReadingFile {
                file,
                last_acked_block,
                timeout_events,
                last_was_final,
                block_size,
            } => {
                Self::handle_reading_file_event(
                    file.clone(),
                    *last_acked_block,
                    *timeout_events,
                    *last_was_final,
                    *block_size,
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

    #[tokio::test]
    async fn read_with_custom_block_size() {
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
                options: vec![
                    (RequestOption {
                        name: "blksize".to_string(),
                        value: "10".to_string(),
                    })
                ]
            }))
            .await
            .unwrap(),
            Response {
                packet: Some(tftp::Packet::OAck {
                    options: vec![
                        (RequestOption {
                            name: "blksize".to_string(),
                            value: "10".to_string(),
                        })
                    ]
                }),
                next_status: ConnectionStatus::WaitingForPacket(DEFAULT_TFTP_TIMEOUT)
            }
        );

        assert_eq!(
            con.handle_event(Event::PacketReceived(tftp::Packet::Ack { block: 0 }))
                .await
                .unwrap(),
            Response {
                packet: Some(tftp::Packet::Data {
                    block: 1,
                    data: file_contents[0..10].to_vec()
                }),
                next_status: ConnectionStatus::WaitingForPacket(DEFAULT_TFTP_TIMEOUT)
            }
        );
    }
}
