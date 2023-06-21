//! This module implements the TFTP protocol in terms of [`simple_proto`].

use std::time::Duration;

use crate::{
    simple_fs,
    simple_proto::{self, ConnectionStatus, Event, Response},
    tftp,
};

use anyhow::Result;

const DEFAULT_TFTP_TIMEOUT: Duration = Duration::from_secs(1);

#[derive(Debug, Clone, Copy)]
enum State {
    WaitingForInitialPacket,
}

/// The current state of the TFTP connection.
#[derive(Debug)]
pub enum Connection<FS: simple_fs::Filesystem> {
    Dead,
    WaitingForInitialPacket { filesystem: FS },
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

    fn handle_initial_event(
        filesystem: &mut FS,
        event: Event<tftp::Packet>,
    ) -> Result<(Self, Response<tftp::Packet>)> {
        match event {
            Event::PacketReceived(p) => match p {
                tftp::Packet::Rrq {
                    filename,
                    mode,
                    options,
                } => todo!(),
                tftp::Packet::Wrq { .. } => Ok((
                    Self::Dead,
                    error_response(
                        2, /* Access violation */
                        "This server only supports reading files",
                    ),
                )),
                packet => Ok((
                    Self::Dead,
                    error_response(0 /* TODO */, "Initial request is not Rrq or Wrq"),
                )),
            },
            Event::Timeout => panic!("Can't receive timeout as initial event"),
        }
    }
}

impl Connection<simple_fs::AsyncFilesystem> {
    pub fn new() -> Self {
        Self::new_with_filesystem(simple_fs::AsyncFilesystem::default())
    }
}

impl<FS: simple_fs::Filesystem> simple_proto::SimpleUdpProtocol for Connection<FS> {
    type Packet = tftp::Packet;
    type Error = anyhow::Error;

    fn handle_event(
        &mut self,
        event: Event<Self::Packet>,
    ) -> Result<simple_proto::Response<Self::Packet>, Self::Error> {
        let (new_self, response) = match self {
            Self::Dead => panic!(
                "Should not receive events on a dead connection: {:?}",
                event
            ),
            Self::WaitingForInitialPacket { filesystem } => {
                Self::handle_initial_event(filesystem, event)?
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

    #[test]
    fn simple_read() {
        let fs =
            simple_fs::MapFilesystem::from([(PathBuf::from_str("/foo").unwrap(), vec![1, 2, 3])]);
        let mut con = Connection::new_with_filesystem(fs);

        assert_eq!(
            con.handle_event(Event::PacketReceived(tftp::Packet::Rrq {
                filename: PathBuf::from("/foo"),
                mode: tftp::RequestMode::Octet,
                options: vec![]
            }))
            .unwrap(),
            Response {
                packet: Some(tftp::Packet::Ack { block: 0 }),
                next_status: ConnectionStatus::WaitingForPacket(DEFAULT_TFTP_TIMEOUT)
            }
        );
    }
}
