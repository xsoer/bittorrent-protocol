#![allow(unused)]
//! Serializable and deserializable protocol messages.

// TODO: Propogate failures to cast values to/from usize

use std::io::{self, Write};

use byteorder::{BigEndian, WriteBytesExt};
use bytes::Bytes;
use nom::{be_u32, be_u8, IResult};

pub use bits_ext::{
    BitsExtensionMessage, ExtendedMessage, ExtendedMessageBuilder, ExtendedType, PortMessage,
};
pub use prot_ext::{
    PeerExtensionProtocolMessage, UtMetadataDataMessage, UtMetadataMessage,
    UtMetadataRejectMessage, UtMetadataRequestMessage,NullProtocolMessage,
};
pub use standard::{
    BitFieldIter, BitFieldMessage, CancelMessage, HaveMessage, PieceMessage, RequestMessage,
};

use super::manager::ManagedMessage;

const KEEP_ALIVE_MESSAGE_LEN: u32 = 0;
const CHOKE_MESSAGE_LEN: u32 = 1;
const UNCHOKE_MESSAGE_LEN: u32 = 1;
const INTERESTED_MESSAGE_LEN: u32 = 1;
const UNINTERESTED_MESSAGE_LEN: u32 = 1;
const HAVE_MESSAGE_LEN: u32 = 5;
const BASE_BITFIELD_MESSAGE_LEN: u32 = 1;
const REQUEST_MESSAGE_LEN: u32 = 13;
const BASE_PIECE_MESSAGE_LEN: u32 = 9;
const CANCEL_MESSAGE_LEN: u32 = 13;

const CHOKE_MESSAGE_ID: u8 = 0;
const UNCHOKE_MESSAGE_ID: u8 = 1;
const INTERESTED_MESSAGE_ID: u8 = 2;
const UNINTERESTED_MESSAGE_ID: u8 = 3;
const HAVE_MESSAGE_ID: u8 = 4;
const BITFIELD_MESSAGE_ID: u8 = 5;
const REQUEST_MESSAGE_ID: u8 = 6;
const PIECE_MESSAGE_ID: u8 = 7;
const CANCEL_MESSAGE_ID: u8 = 8;

const MESSAGE_LENGTH_LEN_BYTES: usize = 4;
const MESSAGE_ID_LEN_BYTES: usize = 1;
const HEADER_LEN: usize = MESSAGE_LENGTH_LEN_BYTES + MESSAGE_ID_LEN_BYTES;
const BASE_PROT_EXTENSION_MESSAGE_LEN: usize = 2;
// Nom has lots of unused warnings atm, keep this here for now.

mod bencode;

mod prot_ext;
mod bits_ext;
mod standard;

/// Enumeration of messages for `PeerWireProtocol`.
#[derive(Debug,PartialEq)]
pub enum PeerWireProtocolMessage
{
    /// Message to keep the connection alive.
    KeepAlive,
    /// Message to tell a peer we will not be responding to their requests.
    ///
    /// Peers may wish to send *Interested and/or KeepAlive messages.
    Choke,
    /// Message to tell a peer we will now be responding to their requests.
    UnChoke,
    /// Message to tell a peer we are interested in downloading pieces from them.
    Interested,
    /// Message to tell a peer we are not interested in downloading pieces from them.
    UnInterested,
    /// Message to tell a peer we have some (validated) piece.
    Have(HaveMessage),
    /// Message to effectively send multiple HaveMessages in a single message.
    ///
    /// This message is only valid when the connection is initiated with the peer.
    BitField(BitFieldMessage),
    /// Message to request a block from a peer.
    Request(RequestMessage),
    /// Message from a peer containing a block.
    Piece(PieceMessage),
    /// Message to cancel a block request from a peer.
    Cancel(CancelMessage),
    /// Extension messages which are activated via the `ExtensionBits` as part of the handshake.
    BitsExtension(BitsExtensionMessage),
    /// Extension messages which are activated via the Extension Protocol.
    ///
    /// In reality, this can be any type that implements `ProtocolMessage` if, for example,
    /// you are running a private swarm where you know all nodes support a given message(s).
    ProtExtension(PeerExtensionProtocolMessage),
}

impl ManagedMessage for PeerWireProtocolMessage {

    fn keep_alive() -> PeerWireProtocolMessage {
        PeerWireProtocolMessage::KeepAlive
    }

    fn is_keep_alive(&self) -> bool {
        match self {
            &PeerWireProtocolMessage::KeepAlive => true,
            _ => false,
        }
    }
}

impl PeerWireProtocolMessage
{
    pub fn bytes_needed(bytes: &[u8]) -> io::Result<Option<usize>> {
        match be_u32(bytes) {
            // We need 4 bytes for the length, plus whatever the length is...
            IResult::Done(_, length) => Ok(Some(MESSAGE_LENGTH_LEN_BYTES + u32_to_usize(length))),
            _ => Ok(None),
        }
    }

    pub fn parse_bytes(
        bytes: Bytes,
        extended: &Option<ExtendedMessage>
    ) -> io::Result<PeerWireProtocolMessage> {
        match parse_message(bytes,extended) {
            IResult::Done(_, result) => result,
            _ => Err(io::Error::new(
                io::ErrorKind::Other,
                "Failed To Parse PeerWireProtocolMessage",
            )),
        }
    }

    pub fn write_bytes<W>(&self, writer: W, extended: &Option<ExtendedMessage>) -> io::Result<()>
    where
        W: Write,
    {
        match self {
            &PeerWireProtocolMessage::KeepAlive => {
                write_length_id_pair(writer, KEEP_ALIVE_MESSAGE_LEN, None)
            }
            &PeerWireProtocolMessage::Choke => {
                write_length_id_pair(writer, CHOKE_MESSAGE_LEN, Some(CHOKE_MESSAGE_ID))
            }
            &PeerWireProtocolMessage::UnChoke => {
                write_length_id_pair(writer, UNCHOKE_MESSAGE_LEN, Some(UNCHOKE_MESSAGE_ID))
            }
            &PeerWireProtocolMessage::Interested => {
                write_length_id_pair(writer, INTERESTED_MESSAGE_LEN, Some(INTERESTED_MESSAGE_ID))
            }
            &PeerWireProtocolMessage::UnInterested => write_length_id_pair(
                writer,
                UNINTERESTED_MESSAGE_LEN,
                Some(UNINTERESTED_MESSAGE_ID),
            ),
            &PeerWireProtocolMessage::Have(ref msg) => msg.write_bytes(writer),
            &PeerWireProtocolMessage::BitField(ref msg) => msg.write_bytes(writer),
            &PeerWireProtocolMessage::Request(ref msg) => msg.write_bytes(writer),
            &PeerWireProtocolMessage::Piece(ref msg) => msg.write_bytes(writer),
            &PeerWireProtocolMessage::Cancel(ref msg) => msg.write_bytes(writer),
            &PeerWireProtocolMessage::BitsExtension(ref ext) => ext.write_bytes(writer),
            &PeerWireProtocolMessage::ProtExtension(ref ext) => {
                ext.write_bytes( writer,extended)
            }
        }
    }

    pub fn message_size(&self) -> usize {
        let message_specific_len = match self {
            &PeerWireProtocolMessage::KeepAlive => KEEP_ALIVE_MESSAGE_LEN as usize,
            &PeerWireProtocolMessage::Choke => CHOKE_MESSAGE_LEN as usize,
            &PeerWireProtocolMessage::UnChoke => UNCHOKE_MESSAGE_LEN as usize,
            &PeerWireProtocolMessage::Interested => INTERESTED_MESSAGE_LEN as usize,
            &PeerWireProtocolMessage::UnInterested => UNINTERESTED_MESSAGE_LEN as usize,
            &PeerWireProtocolMessage::Have(_) => HAVE_MESSAGE_LEN as usize,
            &PeerWireProtocolMessage::BitField(ref msg) => {
                BASE_BITFIELD_MESSAGE_LEN as usize + msg.bitfield().len()
            }
            &PeerWireProtocolMessage::Request(_) => REQUEST_MESSAGE_LEN as usize,
            &PeerWireProtocolMessage::Piece(ref msg) => {
                BASE_PIECE_MESSAGE_LEN as usize + msg.block().len()
            }
            &PeerWireProtocolMessage::Cancel(_) => CANCEL_MESSAGE_LEN as usize,
            &PeerWireProtocolMessage::BitsExtension(ref ext) => ext.message_size(),
            &PeerWireProtocolMessage::ProtExtension(ref ext) =>{
                BASE_PROT_EXTENSION_MESSAGE_LEN + ext.message_size()
            }
        };

        MESSAGE_LENGTH_LEN_BYTES + message_specific_len
    }
}

/// Write a length and optional id out to the given writer.
fn write_length_id_pair<W>(mut writer: W, length: u32, opt_id: Option<u8>) -> io::Result<()>
where
    W: Write,
{
    writer.write_u32::<BigEndian>(length)?;

    if let Some(id) = opt_id {
        writer.write_u8(id)
    } else {
        Ok(())
    }
}

/// Parse the length portion of a message.
///
/// Panics if parsing failed for any reason.
fn parse_message_length(bytes: &[u8]) -> usize {
    if let IResult::Done(_, len) = be_u32(bytes) {
        u32_to_usize(len)
    } else {
        panic!("bittorrent-protocol_peer: Message Length Was Less Than 4 Bytes")
    }
}

/// Panics if the conversion from a u32 to usize is not valid.
fn u32_to_usize(value: u32) -> usize {
    if value as usize as u32 != value {
        panic!("bittorrent-protocol_peer: Cannot Convert u32 To usize, usize Is Less Than 32-Bits")
    }

    value as usize
}

// Since these messages may come over a stream oriented protocol, if a message is incomplete
// the number of bytes needed will be returned. However, that number of bytes is on a per parser
// basis. If possible, we should return the number of bytes needed for the rest of the WHOLE message.
// This allows clients to only re invoke the parser when it knows it has enough of the data.
fn parse_message(
    mut bytes: Bytes,
    extended: &Option<ExtendedMessage>
) -> IResult<(), io::Result<PeerWireProtocolMessage>>
{
    let header_bytes = bytes.clone();

    // Attempt to parse a built in message type, otherwise, see if it is an extension type.
    alt!(
        (),
        ignore_input!(
            switch!(header_bytes.as_ref(), throwaway_input!(tuple!(be_u32, opt!(be_u8))),
                (KEEP_ALIVE_MESSAGE_LEN, None) => value!(
                    Ok(PeerWireProtocolMessage::KeepAlive)
                ) |
                (KEEP_ALIVE_MESSAGE_LEN, Some(0)) => value!(
                    Ok(PeerWireProtocolMessage::KeepAlive)
                ) |
                (CHOKE_MESSAGE_LEN, Some(CHOKE_MESSAGE_ID)) => value!(
                    Ok(PeerWireProtocolMessage::Choke)
                ) |
                (UNCHOKE_MESSAGE_LEN, Some(UNCHOKE_MESSAGE_ID)) => value!(
                    Ok(PeerWireProtocolMessage::UnChoke)
                ) |
                (INTERESTED_MESSAGE_LEN, Some(INTERESTED_MESSAGE_ID)) => value!(
                    Ok(PeerWireProtocolMessage::Interested)
                ) |
                (UNINTERESTED_MESSAGE_LEN, Some(UNINTERESTED_MESSAGE_ID)) => value!(
                    Ok(PeerWireProtocolMessage::UnInterested)
                ) |
                (HAVE_MESSAGE_LEN, Some(HAVE_MESSAGE_ID)) => map!(
                    call!(HaveMessage::parse_bytes, bytes.split_off(HEADER_LEN)),
                    |res_have| res_have.map(|have| PeerWireProtocolMessage::Have(have))
                ) |
                (message_len, Some(BITFIELD_MESSAGE_ID)) => map!(
                    call!(BitFieldMessage::parse_bytes, bytes.split_off(HEADER_LEN), message_len - 1),
                    |res_bitfield| res_bitfield.map(|bitfield| PeerWireProtocolMessage::BitField(bitfield))
                ) |
                (REQUEST_MESSAGE_LEN, Some(REQUEST_MESSAGE_ID)) => map!(
                    call!(RequestMessage::parse_bytes, bytes.split_off(HEADER_LEN)),
                    |res_request| res_request.map(|request| PeerWireProtocolMessage::Request(request))
                ) |
                (message_len, Some(PIECE_MESSAGE_ID)) => map!(
                    call!(PieceMessage::parse_bytes, bytes.split_off(HEADER_LEN), message_len - 1),
                    |res_piece| res_piece.map(|piece| PeerWireProtocolMessage::Piece(piece))
                ) |
                (CANCEL_MESSAGE_LEN, Some(CANCEL_MESSAGE_ID)) => map!(
                    call!(CancelMessage::parse_bytes, bytes.split_off(HEADER_LEN)),
                    |res_cancel| res_cancel.map(|cancel| PeerWireProtocolMessage::Cancel(cancel))
                )
            )
        ) | map!(
            call!(BitsExtensionMessage::parse_bytes, bytes.clone()),
            |res_bits_ext| res_bits_ext
                .map(|bits_ext| PeerWireProtocolMessage::BitsExtension(bits_ext))
        ) | map!(value!(PeerExtensionProtocolMessage::parse_bytes(bytes,extended)), |res_prot_ext| {
            res_prot_ext.map(|prot_ext| PeerWireProtocolMessage::ProtExtension(prot_ext))
        })
    )
}
