//! This module implements data structure for the TFTP protocol.
//!
//! See [RFC 1350](https://datatracker.ietf.org/doc/html/rfc1350) for
//! the basic protocol. [RFC
//! 1782](https://datatracker.ietf.org/doc/html/rfc1782) covers the
//! option extension to the protocol.

use std::{error::Error, ffi::OsStr, fmt::Display, os::unix::prelude::OsStrExt, path::PathBuf};

use nom::{
    bytes::complete::{tag, take_while},
    number::complete::be_u16,
    sequence::{terminated, tuple},
    IResult,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RequestMode {
    Octet,
    Netascii,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RequestOption {
    Unknown { name: String, value: String },
}

/// A TFTP protocol packet
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Packet {
    Rrq {
        filename: PathBuf,
        mode: RequestMode,
        options: Vec<RequestOption>,
    },
    Wrq {
        filename: PathBuf,
        mode: RequestMode,
        options: Vec<RequestOption>,
    },
    Data {
        block: u16,
        data: Vec<u8>,
    },
    Ack {
        block: u16,
    },
    Error {
        error_code: u16,
        error_msg: String,
    },
    OAck {
        options: Vec<RequestOption>,
    },
}

mod opcodes {
    pub const RRQ: u16 = 1;
    pub const WRQ: u16 = 2;
    pub const DATA: u16 = 3;
    pub const ACK: u16 = 4;
    pub const ERROR: u16 = 5;
    pub const OACK: u16 = 6;
}

struct ProtoOption<'a> {
    key: &'a [u8],
    value: &'a [u8],
}

enum ProtoPacket<'a> {
    Rrq {
        filename: &'a [u8],
        mode: &'a [u8],
        options: Vec<ProtoOption<'a>>,
    },
    Wrq {
        filename: &'a [u8],
        mode: &'a [u8],
        options: Vec<ProtoOption<'a>>,
    },
}

fn null_string(input: &[u8]) -> IResult<&[u8], &[u8]> {
    terminated(take_while(|b| b != 0), tag([0]))(input)
}

fn take_rrq(input: &[u8]) -> IResult<&[u8], ProtoPacket> {
    let (input, (filename, mode)) = tuple((null_string, null_string))(input)?; // TODO Options

    Ok((
        input,
        ProtoPacket::Rrq {
            filename,
            mode,
            options: vec![],
        },
    ))
}

fn take_wrq(input: &[u8]) -> IResult<&[u8], ProtoPacket> {
    let (input, (filename, mode)) = tuple((null_string, null_string))(input)?; // TODO Options

    Ok((
        input,
        ProtoPacket::Wrq {
            filename,
            mode,
            options: vec![],
        },
    ))
}

fn packet(input: &[u8]) -> IResult<&[u8], ProtoPacket> {
    let (input, opcode) = be_u16(input)?;

    eprintln!("opcode {}", opcode);

    match opcode {
        opcodes::RRQ => take_rrq(input),
        opcodes::WRQ => take_wrq(input),
        _ => Err(nom::Err::Error(nom::error::Error::new(
            input,
            nom::error::ErrorKind::NoneOf,
        ))),
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ParseError {
    UnrecognizedPacket,
    InvalidString,
    InvalidMode,
}

impl Display for ParseError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ParseError::InvalidString => {
                write!(f, "The packet contained a string that is not valid UTF-8")
            }
            ParseError::UnrecognizedPacket => {
                write!(f, "Failed to parse a packet")
            }
            ParseError::InvalidMode => {
                write!(
                    f,
                    "Failed to parse the packet because of an invalid mode string"
                )
            }
        }
    }
}

impl Error for ParseError {}

fn mode_from_u8(input: &[u8]) -> Result<RequestMode, ParseError> {
    let mode_str = OsStr::from_bytes(input);

    if mode_str.eq_ignore_ascii_case("netascii") {
        Ok(RequestMode::Netascii)
    } else if mode_str.eq_ignore_ascii_case("octet") {
        Ok(RequestMode::Octet)
    } else {
        Err(ParseError::InvalidMode)
    }
}

impl TryFrom<&[u8]> for Packet {
    type Error = ParseError;

    fn try_from(input: &[u8]) -> Result<Self, ParseError> {
        let (_, proto_packet) = packet(input).map_err(|_| ParseError::UnrecognizedPacket)?;

        let packet = match proto_packet {
            ProtoPacket::Rrq {
                filename,
                mode,
                options: _,
            } => Packet::Rrq {
                // We avoid going through String to accept filenames
                // with invalid UTF-8. While the TFTP spec only allows
                // plain ASCII filenames, this is not the reality on a
                // modern Linux system.
                filename: PathBuf::from(OsStr::from_bytes(filename).to_owned()),
                mode: mode_from_u8(mode)?,
                options: vec![],
            },
            ProtoPacket::Wrq {
                filename,
                mode,
                options: _,
            } => Packet::Wrq {
                // See the comment in Rrq above.
                filename: PathBuf::from(OsStr::from_bytes(filename).to_owned()),
                mode: mode_from_u8(mode)?,
                options: vec![],
            },
        };

        Ok(packet)
    }
}

#[cfg(test)]
mod tests {
    use std::str::FromStr;

    // Note this useful idiom: importing names from outer (for mod tests) scope.
    use super::*;

    #[test]
    fn rrq_without_options() {
        assert_eq!(
            Packet::try_from(b"\x00\x01\0octet\0".as_ref()),
            Ok(Packet::Rrq {
                filename: PathBuf::from_str("").unwrap(),
                mode: RequestMode::Octet,
                options: vec![]
            })
        );

        assert_eq!(
            Packet::try_from(b"\x00\x01foo\0NeTAscIi\0".as_ref()),
            Ok(Packet::Rrq {
                filename: PathBuf::from_str("foo").unwrap(),
                mode: RequestMode::Netascii,
                options: vec![]
            })
        );

        assert_eq!(
            Packet::try_from(b"\x00\x01zOo\0oCtet\0".as_ref()),
            Ok(Packet::Rrq {
                filename: PathBuf::from_str("zOo").unwrap(),
                mode: RequestMode::Octet,
                options: vec![]
            })
        )
    }

    #[test]
    fn wrq_without_options() {
        assert_eq!(
            Packet::try_from(b"\x00\x02\0octet\0".as_ref()),
            Ok(Packet::Wrq {
                filename: PathBuf::from_str("").unwrap(),
                mode: RequestMode::Octet,
                options: vec![]
            })
        );

        assert_eq!(
            Packet::try_from(b"\x00\x02foo\0NeTAscIi\0".as_ref()),
            Ok(Packet::Wrq {
                filename: PathBuf::from_str("foo").unwrap(),
                mode: RequestMode::Netascii,
                options: vec![]
            })
        );

        assert_eq!(
            Packet::try_from(b"\x00\x02zOo\0oCtet\0".as_ref()),
            Ok(Packet::Wrq {
                filename: PathBuf::from_str("zOo").unwrap(),
                mode: RequestMode::Octet,
                options: vec![]
            })
        )
    }
}
