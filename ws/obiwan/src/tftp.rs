//! This module implements data structure for the TFTP protocol.
//!
//! See [RFC 1350](https://datatracker.ietf.org/doc/html/rfc1350) for
//! the basic protocol. [RFC
//! 1782](https://datatracker.ietf.org/doc/html/rfc1782) covers the
//! option extension to the protocol.

use std::{
    error::Error,
    ffi::{OsStr, OsString},
    fmt::Display,
    io::Cursor,
    os::unix::prelude::{OsStrExt, OsStringExt},
    path::PathBuf,
};

use binrw::{binrw, helpers::until_eof, BinReaderExt, BinWriterExt, NullString};

/// TFTP error constants as defined by the RFC.
#[allow(dead_code)]
pub mod error {
    pub const UNDEFINED: u16 = 0;
    pub const FILE_NOT_FOUND: u16 = 1;
    pub const ACCESS_VIOLATION: u16 = 2;
    pub const DISK_FULL: u16 = 3;
    pub const ILLEGAL_OPERATION: u16 = 4;
    pub const UNKNOWN_TRANSFER_ID: u16 = 5;
    pub const FILE_EXISTS: u16 = 6;
    pub const NO_SUCH_USER: u16 = 7;
    pub const INVALID_OPTION: u16 = 8;
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RequestMode {
    Octet,
    Netascii,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RequestOption {
    pub name: String,
    pub value: String,
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

#[binrw]
#[brw(big)]
struct ProtoOption {
    name: NullString,
    value: NullString,
}

#[binrw]
#[brw(big)]
enum ProtoPacket {
    #[brw(magic(1u16))]
    Rrq {
        filename: NullString,
        mode: NullString,
        #[br(parse_with = until_eof)]
        options: Vec<ProtoOption>,
    },
    #[brw(magic(2u16))]
    Wrq {
        filename: NullString,
        mode: NullString,
        #[br(parse_with = until_eof)]
        options: Vec<ProtoOption>,
    },
    #[brw(magic(3u16))]
    Data {
        block: u16,
        #[br(parse_with = until_eof)]
        data: Vec<u8>,
    },
    #[brw(magic(4u16))]
    Ack { block: u16 },
    #[brw(magic(5u16))]
    Error {
        error_code: u16,
        error_msg: NullString,
    },
    #[brw(magic(6u16))]
    OAck {
        #[br(parse_with = until_eof)]
        options: Vec<ProtoOption>,
    },
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

fn mode_to_u8(mode: RequestMode) -> Vec<u8> {
    match mode {
        RequestMode::Octet => b"octet".to_vec(),
        RequestMode::Netascii => b"netascii".to_vec(),
    }
}

fn string_from_u8(input: &[u8]) -> Result<String, ParseError> {
    std::str::from_utf8(input)
        .map_err(|_| ParseError::InvalidString)
        .map(|s| s.to_owned())
}

fn option_from_proto(proto_option: &ProtoOption) -> Result<RequestOption, ParseError> {
    Ok(RequestOption {
        name: string_from_u8(&proto_option.name)?,
        value: string_from_u8(&proto_option.value)?,
    })
}

fn options_from_proto(proto_options: &[ProtoOption]) -> Result<Vec<RequestOption>, ParseError> {
    proto_options.iter().map(option_from_proto).collect()
}

fn option_to_proto(option: &RequestOption) -> ProtoOption {
    ProtoOption {
        name: NullString(option.name.clone().into_bytes()),
        value: NullString(option.value.clone().into_bytes()),
    }
}

fn options_to_proto(options: &[RequestOption]) -> Vec<ProtoOption> {
    options.iter().map(option_to_proto).collect()
}

impl Packet {
    pub fn to_vec(&self) -> Vec<u8> {
        let proto_packet = match self {
            Packet::Rrq {
                filename,
                mode,
                options,
            } => ProtoPacket::Rrq {
                filename: NullString(filename.clone().into_os_string().into_vec()),
                mode: NullString(mode_to_u8(*mode)),
                options: options_to_proto(options),
            },
            Packet::Wrq {
                filename,
                mode,
                options,
            } => ProtoPacket::Wrq {
                filename: NullString(filename.clone().into_os_string().into_vec()),
                mode: NullString(mode_to_u8(*mode)),
                options: options_to_proto(options),
            },
            Packet::Data { block, data } => ProtoPacket::Data {
                block: *block,
                data: data.clone(),
            },
            Packet::Ack { block } => ProtoPacket::Ack { block: *block },
            Packet::Error {
                error_code,
                error_msg,
            } => ProtoPacket::Error {
                error_code: *error_code,
                error_msg: error_msg.clone().into(),
            },
            Packet::OAck { options } => ProtoPacket::OAck {
                options: options_to_proto(options),
            },
        };

        let mut cur = Cursor::new(Vec::new());
        cur.write_be(&proto_packet).unwrap();
        cur.into_inner()
    }
}

impl TryFrom<&[u8]> for Packet {
    type Error = ParseError;

    fn try_from(input: &[u8]) -> Result<Self, ParseError> {
        let proto_packet: ProtoPacket = Cursor::new(input)
            .read_be()
            .map_err(|_| ParseError::UnrecognizedPacket)?;

        let packet = match proto_packet {
            ProtoPacket::Rrq {
                filename,
                mode,
                options,
            } => {
                Packet::Rrq {
                    // We avoid going through String to accept filenames
                    // with invalid UTF-8. While the TFTP spec only allows
                    // plain ASCII filenames, this is not the reality on a
                    // modern Linux system.
                    filename: PathBuf::from(OsString::from_vec(filename.to_vec())),
                    mode: mode_from_u8(&mode)?,
                    options: options_from_proto(&options)?,
                }
            }
            ProtoPacket::Wrq {
                filename,
                mode,
                options,
            } => Packet::Wrq {
                // See the comment in Rrq above.
                filename: PathBuf::from(OsString::from_vec(filename.to_vec())),
                mode: mode_from_u8(&mode)?,
                options: options_from_proto(&options)?,
            },
            ProtoPacket::Data { block, data } => Packet::Data { block, data },
            ProtoPacket::Ack { block } => Packet::Ack { block },
            ProtoPacket::Error {
                error_code,
                error_msg,
            } => Packet::Error {
                error_code,
                error_msg: String::from_utf8(error_msg.to_vec())
                    .map_err(|_| ParseError::InvalidString)?,
            },
            ProtoPacket::OAck { options } => Packet::OAck {
                options: options_from_proto(&options)?,
            },
        };

        Ok(packet)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use std::str::FromStr;

    #[test]
    fn parse_rrq_without_options() {
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
    fn serialize_rrq_without_options() {
        assert_eq!(
            (Packet::Rrq {
                filename: PathBuf::from_str("").unwrap(),
                mode: RequestMode::Octet,
                options: vec![]
            })
            .to_vec(),
            b"\x00\x01\0octet\0"
        );

        assert_eq!(
            (Packet::Rrq {
                filename: PathBuf::from_str("zOo").unwrap(),
                mode: RequestMode::Octet,
                options: vec![]
            })
            .to_vec(),
            b"\x00\x01zOo\0octet\0"
        )
    }

    #[test]
    fn parse_rrq_with_options() {
        assert_eq!(
            Packet::try_from(b"\x00\x01\0octet\0key1\0value1\0key2\0value2\0".as_ref()),
            Ok(Packet::Rrq {
                filename: PathBuf::from_str("").unwrap(),
                mode: RequestMode::Octet,
                options: vec![
                    RequestOption {
                        name: "key1".to_string(),
                        value: "value1".to_string()
                    },
                    RequestOption {
                        name: "key2".to_string(),
                        value: "value2".to_string()
                    }
                ]
            })
        );
    }

    #[test]
    fn serialize_rrq_with_options() {
        assert_eq!(
            (Packet::Rrq {
                filename: PathBuf::from_str("").unwrap(),
                mode: RequestMode::Octet,
                options: vec![
                    RequestOption {
                        name: "key1".to_string(),
                        value: "value1".to_string()
                    },
                    RequestOption {
                        name: "key2".to_string(),
                        value: "value2".to_string()
                    }
                ]
            })
            .to_vec(),
            b"\x00\x01\0octet\0key1\0value1\0key2\0value2\0"
        );
    }

    #[test]
    fn parse_wrq_without_options() {
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

    #[test]
    fn serialize_wrq_without_options() {
        assert_eq!(
            (Packet::Wrq {
                filename: PathBuf::from_str("").unwrap(),
                mode: RequestMode::Octet,
                options: vec![]
            })
            .to_vec(),
            b"\x00\x02\0octet\0",
        );

        assert_eq!(
            (Packet::Wrq {
                filename: PathBuf::from_str("foo").unwrap(),
                mode: RequestMode::Netascii,
                options: vec![]
            })
            .to_vec(),
            b"\x00\x02foo\0netascii\0",
        );
    }

    #[test]
    fn parse_wrq_with_options() {
        assert_eq!(
            Packet::try_from(b"\x00\x02\0octet\0key1\0value1\0key2\0value2\0".as_ref()),
            Ok(Packet::Wrq {
                filename: PathBuf::from_str("").unwrap(),
                mode: RequestMode::Octet,
                options: vec![
                    RequestOption {
                        name: "key1".to_string(),
                        value: "value1".to_string()
                    },
                    RequestOption {
                        name: "key2".to_string(),
                        value: "value2".to_string()
                    }
                ]
            })
        );
    }

    #[test]
    fn serialize_wrq_with_options() {
        assert_eq!(
            (Packet::Wrq {
                filename: PathBuf::from_str("").unwrap(),
                mode: RequestMode::Octet,
                options: vec![
                    RequestOption {
                        name: "key1".to_string(),
                        value: "value1".to_string()
                    },
                    RequestOption {
                        name: "key2".to_string(),
                        value: "value2".to_string()
                    }
                ]
            })
            .to_vec(),
            b"\x00\x02\0octet\0key1\0value1\0key2\0value2\0",
        );
    }

    #[test]
    fn parse_data() {
        assert_eq!(
            Packet::try_from(b"\x00\x03\x12\x34hello world".as_ref()),
            Ok(Packet::Data {
                block: 0x1234,
                data: b"hello world".to_vec(),
            })
        )
    }

    #[test]
    fn serialize_data() {
        assert_eq!(
            (Packet::Data {
                block: 0x1234,
                data: b"hello world".to_vec(),
            })
            .to_vec(),
            b"\x00\x03\x12\x34hello world",
        )
    }

    #[test]
    fn parse_ack() {
        assert_eq!(
            Packet::try_from(b"\x00\x04\x12\x34".as_ref()),
            Ok(Packet::Ack { block: 0x1234 })
        )
    }

    #[test]
    fn serialize_ack() {
        assert_eq!(
            (Packet::Ack { block: 0x1234 }).to_vec(),
            b"\x00\x04\x12\x34",
        )
    }

    #[test]
    fn parse_error() {
        assert_eq!(
            Packet::try_from(b"\x00\x05\x01\x02Some error!\0".as_ref()),
            Ok(Packet::Error {
                error_code: 0x0102,
                error_msg: "Some error!".to_owned()
            })
        )
    }

    #[test]
    fn serialize_error() {
        assert_eq!(
            (Packet::Error {
                error_code: 0x0102,
                error_msg: "Some error!".to_owned()
            })
            .to_vec(),
            b"\x00\x05\x01\x02Some error!\0",
        )
    }

    #[test]
    fn parse_oack() {
        assert_eq!(
            Packet::try_from(b"\x00\x06".as_ref()),
            Ok(Packet::OAck { options: vec![] })
        );

        assert_eq!(
            Packet::try_from(b"\x00\x06key1\0value1\0key2\0value2\0".as_ref()),
            Ok(Packet::OAck {
                options: vec![
                    RequestOption {
                        name: "key1".to_string(),
                        value: "value1".to_string()
                    },
                    RequestOption {
                        name: "key2".to_string(),
                        value: "value2".to_string()
                    }
                ]
            })
        );
    }

    #[test]
    fn serialize_oack() {
        assert_eq!((Packet::OAck { options: vec![] }).to_vec(), b"\x00\x06");

        assert_eq!(
            (Packet::OAck {
                options: vec![
                    RequestOption {
                        name: "key1".to_string(),
                        value: "value1".to_string()
                    },
                    RequestOption {
                        name: "key2".to_string(),
                        value: "value2".to_string()
                    }
                ]
            })
            .to_vec(),
            b"\x00\x06key1\0value1\0key2\0value2\0"
        );
    }
}
