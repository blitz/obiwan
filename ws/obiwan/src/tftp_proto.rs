//! This module implements the TFTP protocol in terms of [`simple_proto`].

use crate::{
    simple_fs,
    simple_proto::{self, Event, Response},
    tftp,
};

use anyhow::Result;

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
