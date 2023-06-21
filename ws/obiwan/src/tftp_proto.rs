//! This module implements the TFTP protocol in terms of [`simple_proto`].

use std::time::Duration;

use crate::{
    simple_fs,
    simple_proto::{self, Event, Response},
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
pub struct Connection<FS: simple_fs::Filesystem> {
    filesystem: FS,

    state: State,
}

impl<FS: simple_fs::Filesystem> Connection<FS> {
    pub fn new_with_filesystem(filesystem: FS) -> Self {
        Self {
            filesystem,
            state: State::WaitingForInitialPacket,
        }
    }

    fn handle_initial_event(
        &mut self,
        event: Event<tftp::Packet>,
    ) -> Result<Response<tftp::Packet>> {
        match event {
            simple_proto::Event::PacketReceived(_) => todo!(),
            simple_proto::Event::Timeout => panic!("Can't receive timeout as initial event"),
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
        match self.state {
            State::WaitingForInitialPacket => self.handle_initial_event(event),
        }
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
