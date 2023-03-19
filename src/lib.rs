//! The MAVLink message set.
//!
//! # Message sets and the `Message` trait
//! Each message set has its own module with corresponding data types, including a `MavMessage` enum
//! that represents all possible messages in that message set. The [`Message`] trait is used to
//! represent messages in an abstract way, and each `MavMessage` enum implements this trait (for
//! example, [`ardupilotmega::MavMessage`]). This is then monomorphized to the specific message
//! set you are using in your application at compile-time via type parameters. If you expect
//! ArduPilotMega-flavored messages, then you will need a `MavConnection<ardupilotmega::MavMessage>`
//! and you will receive `ardupilotmega::MavMessage`s from it.
//!
//! Some message sets include others. For example, all message sets except `common` include the
//! common message set. This is represented with extra values in the `MavMessage` enum: a message
//! in the common message set received on an ArduPilotMega connection will be an
//! `ardupilotmega::MavMessage::common(common::MavMessage)`.
//!
//! Please note that if you want to enable a given message set, you must also enable the
//! feature for the message sets that it includes. For example, you cannot use the `ardupilotmega`
//! feature without also using the `uavionix` and `icarous` features.
//!
#![cfg_attr(not(feature = "std"), no_std)]
#![deny(clippy::all)]
#![warn(clippy::use_self)]

use core::result::Result;

#[cfg(feature = "std")]
use std::io::{Read, Write};

#[cfg(feature = "std")]
use byteorder::ReadBytesExt;

#[cfg(feature = "std")]
mod connection;
#[cfg(feature = "std")]
pub use self::connection::{connect, MavConnection};

mod utils;
#[allow(unused_imports)]
use utils::remove_trailing_zeroes;

#[cfg(feature = "serde")]
use serde::{Deserialize, Serialize};

use bytes::{Buf, BufMut};

use crate::error::ParserError;

use crc_any::CRCu16;

// include generate definitions
include!(concat!(env!("OUT_DIR"), "/mod.rs"));

pub mod error;

#[cfg(feature = "embedded")]
mod embedded;
#[cfg(feature = "embedded")]
use embedded::{Read, Write};

/*
pub struct BytesMut<B> {
    buf: B,
}

impl<B: BufMut> BytesMut<B> {
    fn new(buf: B) -> Self {
        Bytesn { buf }
    }

    fn reserve_slice<'a>(&'a mut self, n: usize) -> Option<InitSlice<'a, B>> {
        use std::{ptr, slice};

        unsafe {
            // SAFETY: We are not reading or writing uninitialized bytes
            let uninit = self.buf.chunk_mut().as_mut_ptr();
            ptr::write_bytes(uninit, 0, n);
        }
    }
}

pub struct InitSlice<'a, B: BufMut> {
    bytes_mut: &'a mut BytesMut<B>,
    n: usize,
}

impl<'a, B: BufMut> Deref for InitSlice<'a, B> {
    type Target = [u8];

    fn deref(&self) -> &[u8] {
        unsafe {
            // SAFETY: 
            let uninit = self.bytes_mut.buf.chunk_mut().as_mut_ptr() as *const _;
            slice::from_raw_parts::<'a, u8>(uninit as *const u8, n)
        }
    }
}

impl<'a, B: BufMut> DerefMut for InitSlice<'a, B> {
    fn deref_mut(&mut self) -> &mut [u8] {
        unsafe {
            let uninit = self.bytes_mut.buf.chunk_mut().as_mut_ptr();
            slice::from_raw_parts::<'a, u8>(uninit as *mut u8, n)
        }
    }
}

impl<'a, B: BufMut> Drop for InitSlice<'a, B> {
    fn drop(&mut self) {
        let n = self.n;
        unsafe { self.bytes_mut.buf.advance_mut(n); }
    }
}

impl<B: BufMut> BufMut for BytesMut<B> {
    fn remaining_mut(&self) -> usize {
        self.buf.remaining_mut()
    }

    unsafe fn advance_mut(&mut self, cnt: usize) {
        self.buf.advance_mut(cnt)
    }

    fn chunk_mut(&mut self) -> bytes::UnintSlice {
        self.buf.chunk_mut()
    }
}
*/

pub const MAX_FRAME_SIZE: usize = 280;

pub struct MessageMeta<M: Message> {
    pub id: u32,
    pub name: &'static str,
    pub extra_crc: u8,
    pub serialized_len: u8,
    pub default: M,
}

pub trait MessageInstance<M: Message>: Default {
    fn meta() -> &'static MessageMeta<M>;

    fn serialize_payload(&self, version: MavlinkVersion, payload: &mut [u8]) -> usize;
    fn deserialize_payload(version: MavlinkVersion, payload: &[u8]) -> Result<Self, ParserError>;
}

pub trait Message: Sized + 'static {
    fn meta(&self) -> &'static MessageMeta<Self>;
    fn meta_from_id(id: u32) -> Option<&'static MessageMeta<Self>>;
    fn meta_from_name(name: &str) -> Option<&'static MessageMeta<Self>>;

    /// Serialize **Message** into byte slice and return count of bytes written
    fn serialize(&self, version: MavlinkVersion, bytes: &mut [u8]) -> usize;

    fn deserialize(
        version: MavlinkVersion,
        msgid: u32,
        payload: &[u8],
    ) -> Result<Self, error::ParserError>;
}

/// Metadata from a MAVLink packet header
#[derive(Debug, Copy, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub struct MavHeader {
    pub system_id: u8,
    pub component_id: u8,
    pub sequence: u8,
}

/// Versions of the Mavlink protocol that we support
#[derive(Debug, Copy, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(Serialize))]
#[cfg_attr(feature = "serde", serde(tag = "type"))]
pub enum MavlinkVersion {
    V1,
    V2,
}

/// Message framing marker for mavlink v1
pub const MAV_STX: u8 = 0xFE;

/// Message framing marker for mavlink v2
pub const MAV_STX_V2: u8 = 0xFD;

/// Return a default GCS header, seq is replaced by the connector
/// so it can be ignored. Set `component_id` to your desired component ID.
impl Default for MavHeader {
    fn default() -> Self {
        Self {
            system_id: 255,
            component_id: 0,
            sequence: 0,
        }
    }
}

/// Encapsulation of the Mavlink message and the header,
/// important to preserve information about the sender system
/// and component id
#[derive(Debug, Clone)]
#[cfg_attr(feature = "serde", derive(Serialize))]
pub struct MavFrame<M: Message> {
    pub header: MavHeader,
    pub msg: M,
    pub protocol_version: MavlinkVersion,
}

impl<M: Message> MavFrame<M> {
    /// Create a new frame with given message
    //    pub fn new(msg: MavMessage) -> MavFrame {
    //        MavFrame {
    //            header: MavHeader::get_default_header(),
    //            msg
    //        }
    //    }

    /// Serialize MavFrame into a vector, so it can be sent over a socket, for example.
    pub fn serialize<B: BufMut>(&self, mut buf: B) -> Result<(), error::NotEnoughBytes> {
        let header_size = if self.protocol_version == MavlinkVersion::V1 {
            MAVLinkV1MessageRaw::HEADER_SIZE
        } else {
            MAVLinkV2MessageRaw::HEADER_SIZE
        };
        let serialized_len = self.msg.meta().serialized_len;
        // Assumes no signature
        let msg_size = header_size + serialized_len as usize + 2;
        if buf.remaining_mut() < msg_size {
            return Err(error::NotEnoughBytes);
        }

        // Serialize header
        buf.put_u8(self.header.system_id);
        buf.put_u8(self.header.component_id);
        buf.put_u8(self.header.sequence);

        // Message ID
        match self.protocol_version {
            MavlinkVersion::V2 => {
                let bytes: [u8; 4] = self.msg.meta().id.to_le_bytes();
                buf.put(&bytes[..3]);
            }
            MavlinkVersion::V1 => buf.put_u8(self.msg.meta().id as u8),
        }
        // Serialize message
        let mut payload_buf = [0u8; 255];
        let payload_len = self.msg.serialize(self.protocol_version, &mut payload_buf);
        debug_assert_eq!(payload_len, serialized_len as usize);

        buf.put(&payload_buf[..payload_len]);
        Ok(())
    }

    /// Deserialize MavFrame from a slice that has been received from, for example, a socket.
    pub fn deserialize(version: MavlinkVersion, input: &[u8]) -> Result<Self, ParserError> {
        let mut buf = input;

        let system_id = buf.get_u8();
        let component_id = buf.get_u8();
        let sequence = buf.get_u8();
        let header = MavHeader {
            system_id,
            component_id,
            sequence,
        };

        let msg_id = match version {
            MavlinkVersion::V2 => buf.get_u32_le(),
            MavlinkVersion::V1 => buf.get_u8().into(),
        };

        match M::deserialize(version, msg_id, buf) {
            Ok(msg) => Ok(Self {
                header,
                msg,
                protocol_version: version,
            }),
            Err(err) => Err(err),
        }
    }

    /// Return the frame header
    pub fn header(&self) -> MavHeader {
        self.header
    }
}

pub fn read_versioned_msg<M: Message, R: Read>(
    r: &mut R,
    version: MavlinkVersion,
) -> Result<(MavHeader, M), error::MessageReadError> {
    match version {
        MavlinkVersion::V2 => read_v2_msg(r),
        MavlinkVersion::V1 => read_v1_msg(r),
    }
}

#[derive(Debug, Copy, Clone, PartialEq, Eq)]
// Follow protocol definition: https://mavlink.io/en/guide/serialization.html#v1_packet_format
pub struct MAVLinkV1MessageRaw([u8; 1 + Self::HEADER_SIZE + 255 + 2]);

impl Default for MAVLinkV1MessageRaw {
    fn default() -> Self {
        Self::new()
    }
}

impl MAVLinkV1MessageRaw {
    const HEADER_SIZE: usize = 5;

    pub const fn new() -> Self {
        Self([0; 1 + Self::HEADER_SIZE + 255 + 2])
    }

    #[inline]
    pub fn header(&mut self) -> &[u8] {
        &self.0[1..=Self::HEADER_SIZE]
    }

    #[inline]
    fn mut_header(&mut self) -> &mut [u8] {
        &mut self.0[1..=Self::HEADER_SIZE]
    }

    #[inline]
    pub fn payload_length(&self) -> u8 {
        self.0[1]
    }

    #[inline]
    pub fn sequence(&self) -> u8 {
        self.0[2]
    }

    #[inline]
    pub fn system_id(&self) -> u8 {
        self.0[3]
    }

    #[inline]
    pub fn component_id(&self) -> u8 {
        self.0[4]
    }

    #[inline]
    pub fn message_id(&self) -> u8 {
        self.0[5]
    }

    #[inline]
    pub fn payload(&self) -> &[u8] {
        let payload_length: usize = self.payload_length().into();
        &self.0[(1 + Self::HEADER_SIZE)..(1 + Self::HEADER_SIZE + payload_length)]
    }

    #[inline]
    pub fn checksum(&self) -> u16 {
        let payload_length: usize = self.payload_length().into();
        u16::from_le_bytes([
            self.0[1 + Self::HEADER_SIZE + payload_length],
            self.0[1 + Self::HEADER_SIZE + payload_length + 1],
        ])
    }

    #[inline]
    fn mut_payload_and_checksum(&mut self) -> &mut [u8] {
        let payload_length: usize = self.payload_length().into();
        &mut self.0[(1 + Self::HEADER_SIZE)..(1 + Self::HEADER_SIZE + payload_length + 2)]
    }

    pub fn calculate_crc<M: Message>(&self) -> u16 {
        let payload_length: usize = self.payload_length().into();
        let mut crc_calculator = CRCu16::crc16mcrf4cc();
        crc_calculator.digest(&self.0[1..(1 + Self::HEADER_SIZE + payload_length)]);
        let extra_crc = M::meta_from_id(self.message_id() as u32)
            .map(|meta| meta.extra_crc)
            .unwrap_or(0);

        crc_calculator.digest(&[extra_crc]);
        crc_calculator.get_crc()
    }

    #[inline]
    pub fn has_valid_crc<M: Message>(&self) -> bool {
        self.checksum() == self.calculate_crc::<M>()
    }

    pub fn serialize_message<M: Message>(&mut self, header: MavHeader, message: &M) {
        self.0[0] = MAV_STX;
        let msgid = message.meta().id;

        let payload_buf = &mut self.0[(1 + Self::HEADER_SIZE)..(1 + Self::HEADER_SIZE + 255)];
        let payload_len = message.serialize(MavlinkVersion::V1, payload_buf);

        let header_buf = self.mut_header();
        header_buf.copy_from_slice(&[
            payload_len as u8,
            header.sequence,
            header.system_id,
            header.component_id,
            msgid as u8,
        ]);

        let crc = self.calculate_crc::<M>();
        self.0[(1 + Self::HEADER_SIZE + payload_len)..(1 + Self::HEADER_SIZE + payload_len + 2)]
            .copy_from_slice(&crc.to_le_bytes());
    }
}

/// Return a raw buffer with the mavlink message
/// V1 maximum size is 263 bytes: https://mavlink.io/en/guide/serialization.html
pub fn read_v1_raw_message<R: Read>(
    reader: &mut R,
) -> Result<MAVLinkV1MessageRaw, error::MessageReadError> {
    loop {
        // search for the magic framing value indicating start of mavlink message
        if reader.read_u8()? == MAV_STX {
            break;
        }
    }

    let mut message = MAVLinkV1MessageRaw::new();

    message.0[0] = MAV_STX;
    reader.read_exact(message.mut_header())?;
    reader.read_exact(message.mut_payload_and_checksum())?;

    Ok(message)
}

/// Read a MAVLink v1  message from a Read stream.
pub fn read_v1_msg<M: Message, R: Read>(
    r: &mut R,
) -> Result<(MavHeader, M), error::MessageReadError> {
    loop {
        let message = read_v1_raw_message(r)?;
        if !message.has_valid_crc::<M>() {
            continue;
        }

        return M::deserialize(
            MavlinkVersion::V1,
            u32::from(message.message_id()),
            message.payload(),
        )
        .map(|msg| {
            (
                MavHeader {
                    sequence: message.sequence(),
                    system_id: message.system_id(),
                    component_id: message.component_id(),
                },
                msg,
            )
        })
        .map_err(|err| err.into());
    }
}

const MAVLINK_IFLAG_SIGNED: u8 = 0x01;

#[derive(Debug, Copy, Clone, PartialEq, Eq)]
// Follow protocol definition: https://mavlink.io/en/guide/serialization.html#mavlink2_packet_format
pub struct MAVLinkV2MessageRaw([u8; 1 + Self::HEADER_SIZE + 255 + 2 + Self::SIGNATURE_SIZE]);

impl Default for MAVLinkV2MessageRaw {
    fn default() -> Self {
        Self::new()
    }
}

impl MAVLinkV2MessageRaw {
    const HEADER_SIZE: usize = 9;
    const SIGNATURE_SIZE: usize = 13;

    pub const fn new() -> Self {
        Self([0; 1 + Self::HEADER_SIZE + 255 + 2 + Self::SIGNATURE_SIZE])
    }

    #[inline]
    pub fn header(&mut self) -> &[u8] {
        &self.0[1..=Self::HEADER_SIZE]
    }

    #[inline]
    fn mut_header(&mut self) -> &mut [u8] {
        &mut self.0[1..=Self::HEADER_SIZE]
    }

    #[inline]
    pub fn payload_length(&self) -> u8 {
        self.0[1]
    }

    #[inline]
    pub fn incompatibility_flags(&self) -> u8 {
        self.0[2]
    }

    #[inline]
    pub fn compatibility_flags(&self) -> u8 {
        self.0[3]
    }

    #[inline]
    pub fn sequence(&self) -> u8 {
        self.0[4]
    }

    #[inline]
    pub fn system_id(&self) -> u8 {
        self.0[5]
    }

    #[inline]
    pub fn component_id(&self) -> u8 {
        self.0[6]
    }

    #[inline]
    pub fn message_id(&self) -> u32 {
        u32::from_le_bytes([self.0[7], self.0[8], self.0[9], 0])
    }

    #[inline]
    pub fn payload(&self) -> &[u8] {
        let payload_length: usize = self.payload_length().into();
        &self.0[(1 + Self::HEADER_SIZE)..(1 + Self::HEADER_SIZE + payload_length)]
    }

    #[inline]
    pub fn checksum(&self) -> u16 {
        let payload_length: usize = self.payload_length().into();
        u16::from_le_bytes([
            self.0[1 + Self::HEADER_SIZE + payload_length],
            self.0[1 + Self::HEADER_SIZE + payload_length + 1],
        ])
    }

    fn mut_payload_and_checksum_and_sign(&mut self) -> &mut [u8] {
        let payload_length: usize = self.payload_length().into();

        // Signature to ensure the link is tamper-proof.
        let signature_size = if (self.incompatibility_flags() & 0x01) == MAVLINK_IFLAG_SIGNED {
            Self::SIGNATURE_SIZE
        } else {
            0
        };

        &mut self.0
            [(1 + Self::HEADER_SIZE)..(1 + Self::HEADER_SIZE + payload_length + 2 + signature_size)]
    }

    pub fn calculate_crc<M: Message>(&self) -> u16 {
        let payload_length: usize = self.payload_length().into();
        let mut crc_calculator = CRCu16::crc16mcrf4cc();
        crc_calculator.digest(&self.0[1..(1 + Self::HEADER_SIZE + payload_length)]);

        let extra_crc = M::meta_from_id(self.message_id())
            .map(|meta| meta.extra_crc)
            .unwrap_or(0);
        crc_calculator.digest(&[extra_crc]);
        crc_calculator.get_crc()
    }

    #[inline]
    pub fn has_valid_crc<M: Message>(&self) -> bool {
        self.checksum() == self.calculate_crc::<M>()
    }

    pub fn serialize_message<M: Message>(&mut self, header: MavHeader, message: &M) {
        self.0[0] = MAV_STX_V2;
        let msgid = message.meta().id;
        let msgid_bytes = msgid.to_le_bytes();

        let payload_buf = &mut self.0[(1 + Self::HEADER_SIZE)..(1 + Self::HEADER_SIZE + 255)];
        let payload_len = message.serialize(MavlinkVersion::V2, payload_buf);

        let header_buf = self.mut_header();
        header_buf.copy_from_slice(&[
            payload_len as u8,
            0, //incompat_flags
            0, //compat_flags
            header.sequence,
            header.system_id,
            header.component_id,
            msgid_bytes[0],
            msgid_bytes[1],
            msgid_bytes[2],
        ]);

        let crc = self.calculate_crc::<M>();
        self.0[(1 + Self::HEADER_SIZE + payload_len)..(1 + Self::HEADER_SIZE + payload_len + 2)]
            .copy_from_slice(&crc.to_le_bytes());
    }
}

/// Return a raw buffer with the mavlink message
/// V2 maximum size is 280 bytes: https://mavlink.io/en/guide/serialization.html
pub fn read_v2_raw_message<R: Read>(
    reader: &mut R,
) -> Result<MAVLinkV2MessageRaw, error::MessageReadError> {
    loop {
        // search for the magic framing value indicating start of mavlink message
        if reader.read_u8()? == MAV_STX_V2 {
            break;
        }
    }

    let mut message = MAVLinkV2MessageRaw::new();

    message.0[0] = MAV_STX_V2;
    reader.read_exact(message.mut_header())?;
    reader.read_exact(message.mut_payload_and_checksum_and_sign())?;

    Ok(message)
}

/// Read a MAVLink v2  message from a Read stream.
pub fn read_v2_msg<M: Message, R: Read>(
    read: &mut R,
) -> Result<(MavHeader, M), error::MessageReadError> {
    loop {
        let message = read_v2_raw_message(read)?;
        if !message.has_valid_crc::<M>() {
            // bad crc: ignore message
            continue;
        }

        return M::deserialize(MavlinkVersion::V2, message.message_id(), message.payload())
            .map(|msg| {
                (
                    MavHeader {
                        sequence: message.sequence(),
                        system_id: message.system_id(),
                        component_id: message.component_id(),
                    },
                    msg,
                )
            })
            .map_err(|err| err.into());
    }
}

/// Write a message using the given mavlink version
pub fn write_versioned_msg<M: Message, W: Write>(
    w: &mut W,
    version: MavlinkVersion,
    header: MavHeader,
    data: &M,
) -> Result<usize, error::MessageWriteError> {
    match version {
        MavlinkVersion::V2 => write_v2_msg(w, header, data),
        MavlinkVersion::V1 => write_v1_msg(w, header, data),
    }
}

/// Write a MAVLink v2 message to a Write stream.
pub fn write_v2_msg<M: Message, W: Write>(
    w: &mut W,
    header: MavHeader,
    data: &M,
) -> Result<usize, error::MessageWriteError> {
    let mut message_raw = MAVLinkV2MessageRaw::new();
    message_raw.serialize_message(header, data);

    let payload_length: usize = message_raw.payload_length().into();
    let len = 1 + MAVLinkV2MessageRaw::HEADER_SIZE + payload_length + 2;

    w.write_all(&message_raw.0[..len])?;

    Ok(len)
}

/// Write a MAVLink v1 message to a Write stream.
pub fn write_v1_msg<M: Message, W: Write>(
    w: &mut W,
    header: MavHeader,
    data: &M,
) -> Result<usize, error::MessageWriteError> {
    let mut message_raw = MAVLinkV1MessageRaw::new();
    message_raw.serialize_message(header, data);

    let payload_length: usize = message_raw.payload_length().into();
    let len = 1 + MAVLinkV1MessageRaw::HEADER_SIZE + payload_length + 2;

    w.write_all(&message_raw.0[..len])?;

    Ok(len)
}
