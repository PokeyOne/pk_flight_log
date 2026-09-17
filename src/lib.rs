//! The Pokey Flight Log data logging system.
//!
//! This is a file-system and flight log format writing and reading crate. The
//! file system is designed to be fault and power-loss tolerant, auto wear-
//! leveling, and to work on embedded systems like an RPAS OR UAV.
//!
//! **NOTE: Bad block handling & NAND flash support is not implemented as of
//! v0.2.0, but is planned for v0.3.0**
//!
//! # The Format & File System
//!
//! Data is logged in blocks circularly across the disk. Each block has a
//! sequence number and type. There are two types of blocks: metadata and data.
//! Starting from the beginning of the drive, blocks are logged consecutively,
//! with increasing sequence IDs. At the end of the drive it will wrap around
//! to block 0 again, but the sequence ID will continue incrementing. This is
//! how a reader can know the order of blocks in the file system.
//!
//! Each block has an independently-crc-protected header which allows faster
//! traversal of the file system. A meta data block is used to indicate a new
//! file.
//!
//! Data is logged in packets which contain a time stamp, an ID, and some data.
//! What each packet ID contains is described in the metadata block itself.
//!
//! ## Timestamps
//!
//! All packets are timestamped with a pseudo 64-bit timestamp. Each block will
//! have a full 64-bit timestamp in the chosen units (usually microseconds),
//! and then each packet has a 16-bit unsigned timestamp representing the number
//! of time units since the last timestamp recorded. If it has been more than
//! 2^16-1 time units since the last packet was logged, then a special type of
//! packet called a "time sync" packet is logged containing a full timestamp,
//! and the original data packet is logged with a timestamp of 0.
//!
//! # Fault Tolerance
//!
//! ## Missing or Corrupted Data/Blocks
//!
//! Due to the block nature of the format, if a block cannot be parsed because
//! of either corrupted data, a new bad block, or some sort of other failure,
//! then reader can detect this via CRC and simply skip that block.
//!
//! ## Power Loss
//!
//! A CRC on both the header and the data of each block results in a
//! half-written block being considered invalid. This is a key consideration for
//! the block size, as it will determine how much data can be lost during a
//! power outage (block size is currently hard-coded, but will be changed by
//! v0.3.0).

// TODO: v0.3.0 - NAND & bad blocks
// TODO: v0.3.0 - What happens if the header block is corrupted?
// TODO: v0.3.0 - Adjustable block size and other properties.
// TODO: v0.4.0 - Do we want signed packet timestamps to allow efficient out of order logging?
// TODO: v0.2.1 - Check if timestamp wrapping and out of order is handled properly?

#![no_std]

#[cfg(any(test, feature = "std"))]
extern crate std;

pub mod block;
pub mod crc32;
pub mod metadata;
pub mod packet;
pub mod value;

/// This is the current latest-version block size.
///
/// This matches the erase size of the flash that was used in the original NOR
/// drone project that this was made for. A refactor is needed if this ever does
/// not match the erase size. Future versions will make this configurable and
/// add support for NAND flash.
pub const BLOCK_SIZE: usize = 4096;

/// The version of the format.
#[repr(u8)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Version {
    /// The first version of the format.
    V1 = 0,
    /// The second version of the format.
    ///
    /// This version added a new flag for packet defs on whether they should
    /// be logged to the terminal during decoding.
    V2 = 1,
}

#[cfg(test)]
#[test]
fn test_version_order() {
    assert!(Version::V1 < Version::V2);
}

impl Version {
    pub const LATEST: Version = Version::V2;

    pub fn is_valid_u8(value: u8) -> bool {
        match value {
            x if x == Version::V1 as u8 => true,
            x if x == Version::V2 as u8 => true,
            _ => false,
        }
    }
}
pub const LATEST_VERSION: Version = Version::LATEST;

/// Trait for objects that can be serialized into a binary format.
pub trait BinarySerializable {
    /// Serialize the data into the provided buffer.
    ///
    /// Returns the number of bytes written, or an error if the buffer is too
    /// small.
    ///
    /// Note that while deserialize takes a version, the serialize method
    /// explicitly does not. This is because serialization will always be done
    /// using the latest version of the format.
    fn serialize(&self, buf: &mut [u8]) -> Result<usize, BinarySerializeError>;
}

/// Trait for objects that can be deserialized from a binary format.
///
/// `Sized` is required to be able to return `Self` in the `Result` of the
/// `deserialize` method.
pub trait BinaryDeserializable: Sized {
    /// Deserialize the data from the provided buffer.
    ///
    /// Takes a version parameter so that older data may still be decoded.
    ///
    /// Returns the deserialized object and bytes used, or an error if the
    /// buffer is too small or the data is invalid.
    fn deserialize(buf: &[u8], version: Version) -> Result<(usize, Self), BinaryDeserializeError>;
}

impl BinaryDeserializable for u8 {
    fn deserialize(buf: &[u8], _version: Version) -> Result<(usize, Self), BinaryDeserializeError> {
        if buf.is_empty() {
            return Err(BinaryDeserializeError::BufferTooSmall);
        }
        Ok((1, buf[0]))
    }
}

impl BinaryDeserializable for u16 {
    fn deserialize(buf: &[u8], _version: Version) -> Result<(usize, Self), BinaryDeserializeError> {
        if buf.len() < 2 {
            return Err(BinaryDeserializeError::BufferTooSmall);
        }
        Ok((2, u16::from_le_bytes([buf[0], buf[1]])))
    }
}

impl BinaryDeserializable for u32 {
    fn deserialize(buf: &[u8], _version: Version) -> Result<(usize, Self), BinaryDeserializeError> {
        if buf.len() < 4 {
            return Err(BinaryDeserializeError::BufferTooSmall);
        }
        Ok((4, u32::from_le_bytes([buf[0], buf[1], buf[2], buf[3]])))
    }
}

impl BinaryDeserializable for u64 {
    fn deserialize(buf: &[u8], _version: Version) -> Result<(usize, Self), BinaryDeserializeError> {
        if buf.len() < 8 {
            return Err(BinaryDeserializeError::BufferTooSmall);
        }
        Ok((
            8,
            u64::from_le_bytes([
                buf[0], buf[1], buf[2], buf[3], buf[4], buf[5], buf[6], buf[7],
            ]),
        ))
    }
}

impl BinaryDeserializable for Version {
    fn deserialize(buf: &[u8], version: Version) -> Result<(usize, Self), BinaryDeserializeError> {
        let raw = u8::deserialize(buf, version)?.1;
        match raw {
            _ if raw == (Version::V1 as u8) => Ok((1, Version::V1)),
            _ if raw == (Version::V2 as u8) => Ok((1, Version::V2)),
            _ => Err(BinaryDeserializeError::InvalidVersion),
        }
    }
}

#[derive(Debug, Clone)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub enum BinarySerializeError {
    BufferTooSmall,
}

#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub enum BinaryDeserializeError {
    BufferTooSmall,
    InvalidVersion,
    InvalidBlockType,
    InvalidData,
    UnterminatedFieldName,
    // TODO: Change this so have just a start and end span, no string. Requires
    // more complicated pass through of current location and stuff...
    #[cfg(any(test, feature = "std"))]
    FieldNameTooLong(usize, std::string::String),
    #[cfg(not(any(test, feature = "std")))]
    FieldNameTooLong(usize),
    FieldNameStorageError,
    InvalidValueKind,
}

/// Helper for deserializing multiple objects from a buffer that are also
/// deserializable.
pub struct DeserializeContext<'a> {
    pub buf: &'a [u8],
    pub offset: usize,
    pub version: Version,
}

impl<'a> DeserializeContext<'a> {
    pub fn new(buf: &'a [u8], version: Version) -> Self {
        Self {
            buf,
            offset: 0,
            version,
        }
    }

    /// Deserialize an object of type `T` from the buffer, advancing the offset.
    pub fn deserialize<T: BinaryDeserializable>(&mut self) -> Result<T, BinaryDeserializeError> {
        let (size, obj) = T::deserialize(&self.buf[self.offset..], self.version)?;
        self.offset += size;
        Ok(obj)
    }

    /// Convenience method for `deserialize::<u8>()`.
    pub fn read_u8(&mut self) -> Result<u8, BinaryDeserializeError> {
        let (size, value) = u8::deserialize(&self.buf[self.offset..], self.version)?;
        self.offset += size;
        Ok(value)
    }

    /// Convenience method for `deserialize::<u16>()`.
    pub fn read_u16(&mut self) -> Result<u16, BinaryDeserializeError> {
        let (size, value) = u16::deserialize(&self.buf[self.offset..], self.version)?;
        self.offset += size;
        Ok(value)
    }

    /// Convenience method for `deserialize::<u32>()`.
    pub fn read_u32(&mut self) -> Result<u32, BinaryDeserializeError> {
        let (size, value) = u32::deserialize(&self.buf[self.offset..], self.version)?;
        self.offset += size;
        Ok(value)
    }

    /// Convenience method for `deserialize::<u64>()`.
    pub fn read_u64(&mut self) -> Result<u64, BinaryDeserializeError> {
        let (size, value) = u64::deserialize(&self.buf[self.offset..], self.version)?;
        self.offset += size;
        Ok(value)
    }

    /// Get the total number of bytes deserialized from the start of the buffer.
    pub fn total_offset(&self) -> usize {
        self.offset
    }
}
