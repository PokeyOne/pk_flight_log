//! This module defines the structure of a block, which is the unit of data that
//! is written to flash.
//!
//! The basic structure of a block contains a header and a payload.

use crate::crc32::Crc32Interface;
use crate::metadata::Metadata;
use crate::{
    BLOCK_SIZE, BinaryDeserializable, BinaryDeserializeError, BinarySerializable,
    BinarySerializeError, DeserializeContext, Version,
};

#[cfg(test)]
mod tests;

/// Size of the block header data in bytes.
pub const HEADER_SIZE: u16 = 20;

/// The type of block.
#[repr(u8)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BlockType {
    /// A block containing metadata.
    Meta = 0,
    /// A block containing data.
    Data = 1,
}

impl BinaryDeserializable for BlockType {
    fn deserialize(
        buf: &[u8],
        version: Version,
    ) -> Result<(usize, BlockType), BinaryDeserializeError> {
        let block_type = match u8::deserialize(buf, version)?.1 {
            0 => BlockType::Meta,
            1 => BlockType::Data,
            _ => return Err(BinaryDeserializeError::InvalidBlockType),
        };

        Ok((1, block_type))
    }
}

/// The header of a block.
///
/// This is the very first bytes on the block, which allows the reader to know
/// what the contents of the block should be.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BlockHeader {
    /// Future version.
    ///
    /// Written as one byte.
    pub version: Version,
    /// The type of block.
    ///
    /// Written as one byte.
    pub block_type: BlockType,
    /// Number of valid bytes in the block (after the header).
    ///
    /// Written as two little-endian bytes.
    pub size: u16,
    /// Index of the block on the flash.
    ///
    /// This helps with tracking the last valid block and also helps to find
    /// wrapped blocks.
    ///
    /// 32-bits, means that it will only wrap after 4.3 billion blocks of data.
    /// On the original drone project with 4096 byte blocks, this is about
    /// 18 TB, which would exceed the lifetime of the drone. Additionally, with
    /// only ~8000 blocks on the original drone, this over 500k erase/write
    /// cycles. Above the expected lifetime of the chip.
    ///
    /// TODO: Handle wrapped case properly. Low priority.
    pub block_index: u32,
    /// This is the absolute timestamp since startup of this block.
    ///
    /// Units are arbitrary and should be defined by the system using the pk
    /// flight log system.
    ///
    /// Every packet has a timestamp that is relative to the previous packet,
    /// and every block has this timestamp for synchronization and also prevents
    /// one bad block from messing up everything.
    pub timestamp: u64,
    /// CRC32 of just the header itself.
    ///
    /// This is used when reading the file system so that we can read the
    /// buffer stats without having to read and verify the entire block.
    ///
    /// CRC follows the exact same algorithm as the CRC of the block.
    ///
    /// NOTE: this value is not used when writing the block, as it is
    /// dynamically calculated when the written.
    pub header_crc: u32,
}

impl BlockHeader {
    /// Serialize the block header into the provided buffer.
    ///
    /// Will only return an error if the buffer is too small. Used the CRC
    /// interface to calculate the CRC of the header.
    pub fn serialize(
        &self,
        buf: &mut [u8],
        crc_handler: &mut dyn Crc32Interface,
    ) -> Result<usize, BinarySerializeError> {
        if buf.len() < HEADER_SIZE as usize {
            return Err(BinarySerializeError::BufferTooSmall);
        }

        buf[0] = self.version as u8;
        buf[1] = self.block_type as u8;
        buf[2..4].copy_from_slice(&self.size.to_le_bytes());
        buf[4..8].copy_from_slice(&self.block_index.to_le_bytes());
        buf[8..16].copy_from_slice(&self.timestamp.to_le_bytes());
        let crc = crc_handler.crc32(&buf[..16]);
        buf[16..20].copy_from_slice(&crc.to_le_bytes());

        Ok(HEADER_SIZE as usize)
    }
}

impl BinaryDeserializable for BlockHeader {
    /// Deserialize a block header from the provided buffer.
    ///
    /// Takes a version parameter only due to the `BinaryDeserializable` trait
    /// requiring it, but this method is completely version agnostic as the
    /// block header itself has the version information embedded within it.
    ///
    /// The returned version information should then be respected after
    /// deserialization.
    fn deserialize(
        buf: &[u8],
        _version: Version,
    ) -> Result<(usize, BlockHeader), BinaryDeserializeError> {
        if buf.len() < HEADER_SIZE as usize {
            return Err(BinaryDeserializeError::BufferTooSmall);
        }

        let (version_size, version) = Version::deserialize(buf, Version::LATEST)?;

        let mut ctx = DeserializeContext::new(&buf[version_size..], version);

        let block_type: BlockType = ctx.deserialize()?;
        let size = ctx.read_u16()?;
        let block_index = ctx.read_u32()?;
        let timestamp = ctx.read_u64()?;
        let header_crc = ctx.read_u32()?;

        Ok((
            ctx.total_offset() + version_size,
            BlockHeader {
                version,
                block_type,
                size,
                block_index,
                timestamp,
                header_crc,
            },
        ))
    }
}

/// Wrapper around a mutable reference to a block buffer which allows writing
/// data to the block.
pub struct BlockWriter<'a> {
    buf: &'a mut [u8; BLOCK_SIZE],
    used: u16,
    /// The block header that will be written to the block when finished.
    header: BlockHeader,
}

impl<'a> BlockWriter<'a> {
    /// Create a new block writer with the given buffer and header.
    ///
    /// The header is written to the buffer, and the used bytes are set to the
    /// size of the header.
    pub fn new(buf: &'a mut [u8; BLOCK_SIZE], header: BlockHeader) -> Self {
        // used is set to HEADER_SIZE even though the header is not written yet
        // but it will be written at the end. Not written initially because the
        // valid data length field of the header is not known yet.
        Self {
            buf,
            used: HEADER_SIZE,
            header,
        }
    }

    /// Write some data to the block.
    ///
    /// Returns an error if there is not enough space in the block.
    pub fn write(&mut self, data: &[u8]) -> Result<(), BinarySerializeError> {
        if data.len() > self.space() {
            return Err(BinarySerializeError::BufferTooSmall);
        }

        let start = self.used as usize;
        let end = start + data.len();
        self.buf[start..end].copy_from_slice(data);
        self.used += data.len() as u16;

        Ok(())
    }

    /// Get the number of data bytes that can still be written to the block.
    ///
    /// This automatically accounts for the header and CRC.
    pub fn space(&self) -> usize {
        // Just prevent any potential subtraction underflow. Should in theory
        // be guaranteed to never happen. Maybe with more unit tests we could
        // make this a debug_assert! instead of a runtime check.
        #[cfg(test)]
        assert!(
            self.used + 4 <= BLOCK_SIZE as u16,
            "BlockWriter used bytes + 4 is greater than BLOCK_SIZE"
        );

        // -4 for CRC
        BLOCK_SIZE.saturating_sub(self.used as usize + 4)
    }

    /// Write the metadata to this block.
    ///
    /// This will update the block type to meta and serialize the provided
    /// metadata into the block.
    ///
    /// Returns an error if there is not enough space in the block for the
    /// metadata, or if there is some other serialization error in the metadata.
    pub fn write_meta_block(&mut self, meta: &Metadata) -> Result<(), BinarySerializeError> {
        self.header.block_type = BlockType::Meta;

        let meta_size = meta.serialize(&mut self.buf[self.used as usize..BLOCK_SIZE - 4])?;
        self.used += meta_size as u16;
        Ok(())
    }

    /// Finish writing to the block and return the number of bytes written.
    ///
    /// This writes things like the number of valid content bytes, plus the CRC
    /// at the end of the block.
    pub fn finish<T: Crc32Interface>(mut self, crc_interface: &mut T) -> &'a mut [u8; BLOCK_SIZE] {
        // Update header.
        self.header.size = self.used - HEADER_SIZE;
        // Unwrap considered safe here because header is guaranteed to be
        // smaller than the block size and we are giving it a whole block
        // buffer. Plus the space was already accounted for in the used variable.
        self.header
            .serialize(&mut self.buf[..], crc_interface)
            .unwrap();

        // Update CRC
        let start_of_data = HEADER_SIZE as usize;
        let end_of_data = HEADER_SIZE as usize + self.header.size as usize;
        let crc = crc_interface.crc32(&self.buf[start_of_data..end_of_data]);
        self.buf[end_of_data..(end_of_data + 4)].copy_from_slice(&crc.to_le_bytes());

        self.buf
    }
}
