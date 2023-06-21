//! This module contains data structure to model a simple UDP
//! protocol.
//!
//! The abstraction aims to make unit testing for simple UDP protocols
//! easy.

use std::{fmt::Debug, time::Duration};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConnectionStatus {
    // The connection is terminated.
    Terminated,

    // We are waiting for a packet to arrive in the given
    // timeframe. If it doesn't a timeout event is generated.
    WaitingForPacket(Duration),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Event<T: Debug + Clone + PartialEq + Eq> {
    // The given packet was received on this connection.
    PacketReceived(T),

    // We timed out waiting for a packet to arrive.
    Timeout,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Response<T: Debug + Clone + PartialEq + Eq> {
    pub packet: Option<T>,
    pub next_status: ConnectionStatus,
}

pub trait SimpleUdpProtocol {
    type Packet: Debug + Clone + PartialEq + Eq;
    type Error;

    fn handle_event(
        &mut self,
        event: Event<Self::Packet>,
    ) -> Result<Response<Self::Packet>, Self::Error>;
}
