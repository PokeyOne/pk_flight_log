use crate::metadata::{FieldName, PacketDef, ValueDef, ValueKind};
use crate::value::PacketValue;
use crate::{BinaryDeserializeError, BinarySerializeError};

#[cfg(test)]
mod tests;

pub type PacketID = u8;
pub type PacketTimestamp = u16;
pub const PACKET_HEADER_SIZE: usize = size_of::<PacketID>() + size_of::<PacketTimestamp>();

/// Header information stored in a packet.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PacketHeader {
    pub id: PacketID,
    pub timestamp: PacketTimestamp,
}

impl PacketHeader {
    pub fn new(id: PacketID, timestamp: PacketTimestamp) -> Self {
        Self { id, timestamp }
    }
}

/// Reserved packet ID for timesync packets.
pub const TIME_SYNC_PACKET_ID: PacketID = 0;

// TODO: Make including the time sync packet definition optional.

/// This is the packet definition that must be included for the time sync
/// packet.
pub const TIME_SYNC_PACKET_DEF: PacketDef = PacketDef {
    id: TIME_SYNC_PACKET_ID,
    name: FieldName::from_array(*b"timesync"),
    log_to_terminal: false,
    values: heapless::Vec::from_array([ValueDef {
        name: FieldName::from_array(*b"time_us"),
        kind: ValueKind::U64,
        count: 1,
    }]),
};

// TODO: Some of this packet writer stuff is a bit clunky.

pub struct PacketWriter<'a> {
    buf: &'a mut [u8],
    used: usize,
    /// Number of bits in the sub-byte-bits used.
    ///
    /// Sub-byte bits are actually stored in the last used value in the buffer.
    /// This is done so that the buffer is always kept up to date and never
    /// has to be flushed. It also ensures by design that the buf always has
    /// space for the partial.
    sub_byte_bits_used: u8,
}

impl<'a> PacketWriter<'a> {
    /// Create a new PacketWriter that will write to the provided buffer.
    ///
    /// # Params
    ///
    /// * `buf` - The buffer to write the packet to. Must be at least 3 bytes in length.
    /// * `header` - The header of the packet, containing the ID and timestamp.
    ///   The ID will be written to the first byte of the buffer, and the
    ///   timestamp will be written to bytes 1-2 in little-endian format.
    ///
    /// Timestamp should be relative to the last packet written, except for the
    /// first packet in the block, which is relative to the start of the block.
    ///
    /// # Returns
    ///
    /// Will return None if the buffer is too small to write the packet header.
    /// (3 bytes: 1 for packet ID, 2 for timestamp)
    pub fn new(buf: &'a mut [u8], header: PacketHeader) -> Option<Self> {
        if buf.len() < 3 {
            return None;
        }

        buf[0] = header.id;
        let mut result = Self {
            buf,
            used: 3,
            sub_byte_bits_used: 0,
        };

        result.set_timestamp(header.timestamp);

        Some(result)
    }

    /// Set the timestamp of this packet.
    pub fn set_timestamp(&mut self, timestamp: u16) {
        self.buf[1..3].copy_from_slice(&timestamp.to_le_bytes());
    }

    /// True if sub-byte values are currently being written.
    ///
    /// Consecutive booleans are packed as bits. This method tells you that the
    /// latest byte has 1 to 7 booleans (inclusive) written.
    fn has_sub_byte_values(&self) -> bool {
        self.sub_byte_bits_used > 0
    }

    /// Check how many bytes of space are remaining.
    ///
    /// # Panics
    ///
    /// If a test build, will panic if `self.used` is larger than the buffer
    /// length, as this should only happen via internal errors. In production
    /// it will just handle it by capping to 0 left.
    pub fn space(&self) -> usize {
        #[cfg(test)]
        assert!(
            self.buf.len() >= self.used,
            "PacketWriter used bytes is greater than or equal to buffer length"
        );

        self.buf.len().saturating_sub(self.used)
    }

    /// Commits any sub-byte values that have not been committed to storage.
    ///
    /// This should basically be called for any value that is not a sub-byte
    /// value, because all byte or larger values are written byte-aligned.
    fn commit_to_the_bit(&mut self) {
        // Because the in-progress byte is stored at the end of the buffer,
        // which is already included in the "used" counter, all we need to do
        // is reset the number of bits in the latest partial.
        self.sub_byte_bits_used = 0;
    }

    /// Write a single u8 value to the packet, and return an error if out of
    /// space.
    pub fn write_u8(&mut self, val: u8) -> Result<(), BinarySerializeError> {
        self.commit_to_the_bit();

        if self.space() < 1 {
            return Err(BinarySerializeError::BufferTooSmall);
        }

        self.buf[self.used] = val;
        self.used += 1;

        Ok(())
    }

    pub fn write_slice(&mut self, slice: &[u8]) -> Result<(), BinarySerializeError> {
        self.commit_to_the_bit();

        if self.space() < slice.len() {
            return Err(BinarySerializeError::BufferTooSmall);
        }

        self.buf[self.used..(self.used + slice.len())].copy_from_slice(slice);
        self.used += slice.len();

        Ok(())
    }

    pub fn write_u16(&mut self, val: u16) -> Result<(), BinarySerializeError> {
        self.write_slice(&val.to_le_bytes())
    }

    pub fn write_u32(&mut self, val: u32) -> Result<(), BinarySerializeError> {
        self.write_slice(&val.to_le_bytes())
    }

    pub fn write_u64(&mut self, val: u64) -> Result<(), BinarySerializeError> {
        self.write_slice(&val.to_le_bytes())
    }

    pub fn write_f32(&mut self, val: f32) -> Result<(), BinarySerializeError> {
        self.write_slice(&val.to_le_bytes())
    }

    /// Write a boolean value to the packet data.
    ///
    /// Values are packed MSb-LSb but right-aligned. So for example,
    /// one boolean value will have a mask of 0b0000_0001, and two would be
    /// 0b0000_0011 where the first one added is 0b0010 and the second is
    /// 0b0001.
    ///
    /// This method will handle all bit-packing logic for sub-byte values.
    pub fn write_bool(&mut self, val: bool) -> Result<(), BinarySerializeError> {
        if !self.has_sub_byte_values() {
            // Writing a zero byte will claim a new byte and zero it, which is
            // coincidentally exactly what we want to do here.
            self.write_u8(0)?;
        }

        self.buf[self.used - 1] = (self.buf[self.used - 1] << 1) | (val as u8);
        self.sub_byte_bits_used += 1;

        if self.sub_byte_bits_used == 8 {
            self.commit_to_the_bit();
        }

        Ok(())
    }
}

/// Utility for reading packet values from a buffer.
///
/// Knows nothing about the specific meta for a packet or anything, just gives
/// the caller methods for things like `read_u8`, etc.
pub struct PacketValueReader<'a> {
    buf: &'a [u8],
    used: usize,
    header: PacketHeader,
}

impl<'a> PacketValueReader<'a> {
    /// Create a new PacketValueReader that will read from the provided buffer.
    ///
    /// This will also read the packet header from the beginning of the buffer
    /// and return the result.
    pub fn new(buf: &'a [u8]) -> Result<(Self, PacketHeader), BinaryDeserializeError> {
        if buf.len() < 3 {
            return Err(BinaryDeserializeError::BufferTooSmall);
        }

        let header = PacketHeader {
            id: buf[0],
            timestamp: u16::from_le_bytes([buf[1], buf[2]]),
        };

        Ok((
            Self {
                buf,
                used: 3,
                header,
            },
            header,
        ))
    }

    pub fn used(&self) -> usize {
        self.used
    }

    /// Consume a single byte from the buffer and return it.
    fn read_u8(&mut self) -> Result<u8, BinaryDeserializeError> {
        if self.buf.len() < self.used + 1 {
            return Err(BinaryDeserializeError::BufferTooSmall);
        }

        let val = self.buf[self.used];
        self.used += 1;

        Ok(val)
    }

    /// Read data into a slice.
    ///
    /// Either all of the slice will be filled, or an error will be returned
    /// [`BinaryDeserializeError::BufferTooSmall`].
    fn read_slice(&mut self, slice: &mut [u8]) -> Result<(), BinaryDeserializeError> {
        if self.buf.len() < self.used + slice.len() {
            return Err(BinaryDeserializeError::BufferTooSmall);
        }

        slice.copy_from_slice(&self.buf[self.used..(self.used + slice.len())]);
        self.used += slice.len();

        Ok(())
    }

    fn read_u16(&mut self) -> Result<u16, BinaryDeserializeError> {
        let mut buf = [0u8; 2];
        self.read_slice(&mut buf)?;
        Ok(u16::from_le_bytes(buf))
    }

    fn read_u32(&mut self) -> Result<u32, BinaryDeserializeError> {
        let mut buf = [0u8; 4];
        self.read_slice(&mut buf)?;
        Ok(u32::from_le_bytes(buf))
    }

    fn read_u64(&mut self) -> Result<u64, BinaryDeserializeError> {
        let mut buf = [0u8; 8];
        self.read_slice(&mut buf)?;
        Ok(u64::from_le_bytes(buf))
    }

    fn read_f32(&mut self) -> Result<f32, BinaryDeserializeError> {
        let mut buf = [0u8; 4];
        self.read_slice(&mut buf)?;
        Ok(f32::from_le_bytes(buf))
    }

    fn read_i8(&mut self) -> Result<i8, BinaryDeserializeError> {
        let val = self.read_u8()?;
        Ok(i8::from_le_bytes([val]))
    }

    fn read_i16(&mut self) -> Result<i16, BinaryDeserializeError> {
        let mut buf = [0u8; 2];
        self.read_slice(&mut buf)?;
        Ok(i16::from_le_bytes(buf))
    }

    fn read_i32(&mut self) -> Result<i32, BinaryDeserializeError> {
        let mut buf = [0u8; 4];
        self.read_slice(&mut buf)?;
        Ok(i32::from_le_bytes(buf))
    }

    /// Reads the values of this packet based on the provided metadata
    /// information.
    ///
    /// The given packet defs should be all the expected types of packets, and
    /// this reader will find the corresponding packet definition based on the
    /// already read packet ID.
    pub fn read_values(
        self,
        packet_defs: &'a [PacketDef],
    ) -> Result<PacketValueDefReader<'a>, BinaryDeserializeError> {
        // Finds the appropriate packet def, then gets all its value defs.
        let value_defs = packet_defs
            .iter()
            .find(|x| x.id == self.header.id)
            .ok_or(BinaryDeserializeError::InvalidData)?
            .values
            .as_slice();

        Ok(PacketValueDefReader::new(self, value_defs))
    }
}

/// Wraps a [`PacketValueReader`] to parse a set of `ValueDef`s from a packet
/// def.
pub struct PacketValueDefReader<'a> {
    value_reader: PacketValueReader<'a>,
    /// The value defs in question.
    value_defs: &'a [ValueDef],
    /// The index of the next value def to read from the value reader.
    ///
    /// May be equal to the length of `value_defs` if all value defs have been
    /// read.
    value_def_index: usize,
    /// The current value def being processed, along with its remaining count.
    ///
    /// This is for value defs that have a count higher than 1.
    current_value_def: Option<(&'a ValueDef, u8)>,
    /// Temporary storage for the current byte of sub-byte values.
    sub_byte_bits: u8,
    /// The number of bits left in `sub_byte_bits` to be consumed.
    sub_byte_bits_left: u8,
    /// The number of sub-byte bits remaining after the current value.
    sub_byte_bits_after_current: u32,
}

impl<'a> PacketValueDefReader<'a> {
    /// Creates a new `PacketValueDefReader` with the given value reader and
    /// value defs.
    ///
    /// This is usually called from [`PacketValueReader::read_values`] instead
    /// of directly through this method.
    pub fn new(value_reader: PacketValueReader<'a>, value_defs: &'a [ValueDef]) -> Self {
        Self {
            value_reader,
            value_defs,
            value_def_index: 0,
            current_value_def: None,
            sub_byte_bits: 0,
            sub_byte_bits_left: 0,
            sub_byte_bits_after_current: 0,
        }
    }

    /// Get a reference to the underlying `PacketValueReader`.
    pub fn value_reader(&self) -> &PacketValueReader<'a> {
        &self.value_reader
    }

    /// Get the next value def to parse.
    ///
    /// This already handles the `count` field of the def. e.g. if a value def
    /// comes through with a count of 3, this method will return it three times
    /// before going to the next one.
    fn next_value_def(&mut self) -> Option<&'a ValueDef> {
        if let Some((value_def, remaining)) = self.current_value_def
            && remaining != 0
        {
            self.current_value_def = Some((value_def, remaining - 1));
            return Some(value_def);
        }
        // Continue on if the remaining was 0 because 0 is nothing left.

        loop {
            if self.value_def_index >= self.value_defs.len() {
                return None;
            }

            let value_def = &self.value_defs[self.value_def_index];
            self.value_def_index += 1;

            self.current_value_def = match value_def.count {
                // This value def has no values, look for the next one.
                0 => continue,
                // Don't need to set the current value def because it only has
                // one value.
                1 => None,
                // Set the current value def with n-1 so that the next call will
                // keep returning the rest.
                _ => Some((value_def, value_def.count - 1)),
            };

            return Some(value_def);
        }
    }
}

impl<'a> Iterator for PacketValueDefReader<'a> {
    type Item = Result<PacketValue, BinaryDeserializeError>;

    /// Get the next packet value to parse.
    ///
    /// Returns `None` if at the end of the given buffer, and `Some(Err(_))` if
    /// there are still bytes but some error in parsing the bytes occurs.
    fn next(&mut self) -> Option<Result<PacketValue, BinaryDeserializeError>> {
        loop {
            if self.sub_byte_bits_left == 0 && self.sub_byte_bits_after_current > 0 {
                let bits_to_take = self.sub_byte_bits_after_current.min(8);
                self.sub_byte_bits_after_current -= bits_to_take;

                self.sub_byte_bits_left = bits_to_take as u8;
                let next_val = match self.value_reader.read_u8() {
                    Ok(v) => v,
                    Err(e) => return Some(Err(e)),
                };
                // Bits are stored right-aligned in the byte, with MSB being the
                // first. So need to flip the bits and shift to read out in order
                // written.
                self.sub_byte_bits = next_val.reverse_bits() >> (8 - bits_to_take);
            }

            if self.sub_byte_bits_left > 0 {
                let value = (self.sub_byte_bits & 0b0000_0001) != 0;
                self.sub_byte_bits >>= 1;
                self.sub_byte_bits_left -= 1;

                return Some(Ok(PacketValue::Bool(value)));
            }

            let value_def = self.next_value_def()?;

            let r = match value_def.kind {
                ValueKind::U8 => self.value_reader.read_u8().map(PacketValue::U8),
                ValueKind::U16 => self.value_reader.read_u16().map(PacketValue::U16),
                ValueKind::U32 => self.value_reader.read_u32().map(PacketValue::U32),
                ValueKind::U64 => self.value_reader.read_u64().map(PacketValue::U64),
                ValueKind::F32 => self.value_reader.read_f32().map(PacketValue::F32),
                ValueKind::I8 => self.value_reader.read_i8().map(PacketValue::I8),
                ValueKind::I16 => self.value_reader.read_i16().map(PacketValue::I16),
                ValueKind::I32 => self.value_reader.read_i32().map(PacketValue::I32),
                ValueKind::Bool => {
                    self.sub_byte_bits_after_current = self
                        .current_value_def
                        .take()
                        .map_or(0, |(_, rem)| rem as u32)
                        + 1;

                    // Append all consecutive bit values to the count and
                    // consume them from the iterator
                    while self.value_def_index < self.value_defs.len()
                        && self.value_defs[self.value_def_index].kind == ValueKind::Bool
                    {
                        self.sub_byte_bits_after_current +=
                            self.value_defs[self.value_def_index].count as u32;
                        self.value_def_index += 1;
                    }

                    // Continue the loop, which will then handle the sub
                    // byte bits after current properly.
                    continue;
                }
            };

            return Some(r);
        }
    }
}
