//! This module contains data structure to model a simple UDP
//! protocol.
//!
//! The abstraction aims to make unit testing for simple UDP protocols
//! easy.

use std::time::Duration;

pub enum ConnectionStatus {
    // The connection is terminated.
    Terminated,

    // We are waiting for a packet to arrive in the given
    // timeframe. If it doesn't a timeout event is generated.
    WaitingForPacket(Duration),
}

pub enum Event<T> {
    // The given packet was received on this connection.
    PacketReceived(T),

    // We timed out waiting for a packet to arrive.
    Timeout,
}

pub struct Response<T> {
    packet: Option<T>,
    next_status: ConnectionStatus,
}

pub trait SimpleUdpProtocol {
    type Packet;
    type Error;

    fn handle_event(event: Event<Self::Packet>) -> Result<Response<Self::Packet>, Self::Error>;
}
