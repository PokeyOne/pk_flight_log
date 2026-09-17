use crate::{
    BinaryDeserializable, BinaryDeserializeError, BinarySerializable, BinarySerializeError, Version,
};

#[cfg(test)]
mod tests;

// TODO: Change the way meta data is handled so that it is just given a block
// of memory and you write each definition to it. This allows better validation
// and removes the constraint of having a [`METADATA_MAX_PACKET_DEFS`].

/// The maximum number of packet type definitions that may be included in a
/// metadata block.
pub const METADATA_MAX_PACKET_DEFS: usize = 16;
/// The maximum length of a field name in bytes, not including the null
/// terminator.
pub const FIELD_NAME_MAX_LEN: usize = 8;

/// The metadata that is stored in a block of type [`crate::block::BlockType::Meta`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Metadata {
    /// The packet definitions that are stored in the metadata block.
    pub packet_defs: heapless::Vec<PacketDef, METADATA_MAX_PACKET_DEFS>,
}

impl Metadata {
    /// Create a new empty metadata block.
    pub fn empty() -> Self {
        Self {
            packet_defs: heapless::Vec::new(),
        }
    }

    /// Find a packet definition by its ID.
    ///
    /// Returns `Some(&PacketDef)` if a packet with the given ID exists, or
    /// `None` otherwise.
    pub fn find_packet_def(&self, id: u8) -> Option<&PacketDef> {
        self.packet_defs.iter().find(|def| def.id == id)
    }
}

impl BinarySerializable for Metadata {
    fn serialize(&self, buf: &mut [u8]) -> Result<usize, BinarySerializeError> {
        // NOTE: The metadata does not have a version here because the block
        // header has the version.

        if buf.is_empty() {
            return Err(BinarySerializeError::BufferTooSmall);
        }
        buf[0] = self.packet_defs.len() as u8;

        let mut offset = 1;

        for packet_def in self.packet_defs.iter() {
            let packet_size = packet_def.serialize(&mut buf[offset..])?;
            offset += packet_size;
        }

        Ok(offset)
    }
}

impl BinaryDeserializable for Metadata {
    fn deserialize(
        buf: &[u8],
        version: Version,
    ) -> Result<(usize, Metadata), BinaryDeserializeError> {
        let (mut offset, num_packet_defs) = u8::deserialize(buf, version)?;

        let mut packet_defs = heapless::Vec::<PacketDef, METADATA_MAX_PACKET_DEFS>::new();

        for _ in 0..num_packet_defs {
            let (size, packet_def) = PacketDef::deserialize(&buf[offset..], version)?;
            offset += size;

            packet_defs
                .push(packet_def)
                .map_err(|_| BinaryDeserializeError::BufferTooSmall)?;
        }

        Ok((offset, Metadata { packet_defs }))
    }
}

/// Maximum number of values that can be defined in a [`PacketDef`]
pub const PACKET_DEF_MAX_VALUES: usize = 32;

/// Definition of a packet in data.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PacketDef {
    /// The ID of the packet.
    ///
    /// Must be unique across all packets in the metadata block/file.
    pub id: u8,
    /// The name of the packet.
    ///
    /// On the decoded output data, names of each column will be something like
    /// `packet_name/value_name[index]` for each value in the packet.
    pub name: FieldName,
    /// When true, this packet should be output to terminal and highlighted
    /// when decoded.
    ///
    /// This is intended for packets that are very infrequent or may be better
    /// interpretted in a text log than a graph. Examples may include certain
    /// errors, a panic record, or the reason for the previous reset.
    pub log_to_terminal: bool,
    /// The values that are part of the packet.
    pub values: heapless::Vec<ValueDef, PACKET_DEF_MAX_VALUES>,
}

impl BinarySerializable for PacketDef {
    fn serialize(&self, buf: &mut [u8]) -> Result<usize, BinarySerializeError> {
        let mut offset = 0;

        if buf.is_empty() {
            return Err(BinarySerializeError::BufferTooSmall);
        }
        buf[offset] = self.id;
        offset += 1;

        let name_size = self.name.serialize(&mut buf[offset..])?;
        offset += name_size;

        if buf.len() < offset + 2 {
            return Err(BinarySerializeError::BufferTooSmall);
        }
        buf[offset] = self.log_to_terminal as u8;
        offset += 1;

        buf[offset] = self.values.len() as u8;
        offset += 1;

        for value in self.values.iter() {
            let value_size = value.serialize(&mut buf[offset..])?;
            offset += value_size;
        }

        Ok(offset)
    }
}

impl BinaryDeserializable for PacketDef {
    fn deserialize(
        buf: &[u8],
        version: Version,
    ) -> Result<(usize, PacketDef), BinaryDeserializeError> {
        let (mut offset, id) = u8::deserialize(buf, version)?;

        let (name_size, name) = FieldName::deserialize(&buf[offset..], version)?;
        offset += name_size;

        let log_to_terminal = if version >= Version::V2 {
            let (log_to_terminal_size, log_to_terminal) = u8::deserialize(&buf[offset..], version)?;
            offset += log_to_terminal_size;

            log_to_terminal != 0
        } else {
            // V1 did not have a log_to_terminal field, so default to false.
            false
        };

        let (num_values_size, num_values) = u8::deserialize(&buf[offset..], version)?;
        offset += num_values_size;

        let mut values = heapless::Vec::<ValueDef, PACKET_DEF_MAX_VALUES>::new();
        for _ in 0..num_values {
            let (size, value) = ValueDef::deserialize(&buf[offset..], version)?;
            offset += size;
            values
                .push(value)
                .map_err(|_| BinaryDeserializeError::BufferTooSmall)?;
        }

        Ok((
            offset,
            PacketDef {
                id,
                name,
                log_to_terminal,
                values,
            },
        ))
    }
}

/// A string of up to 8 UTF-8 bytes, not including the null terminator for the
/// name of a value or packet.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct FieldName {
    /// The stored name of the field.
    name: heapless::Vec<u8, FIELD_NAME_MAX_LEN>,
}

impl FieldName {
    /// Initialize the name from an array of u8 characters.
    ///
    /// A compile-time error will be thrown if array length M is greater than
    /// [`FIELD_NAME_MAX_LEN`].
    pub const fn from_array<const M: usize>(arr: [u8; M]) -> Self {
        Self {
            name: heapless::Vec::from_array(arr),
        }
    }

    pub fn from_str_truncating(s: &str) -> Self {
        let mut end = s.len().min(FIELD_NAME_MAX_LEN);
        while !s.is_char_boundary(end) {
            end -= 1;
        }
        Self {
            name: heapless::Vec::from_slice(&s.as_bytes()[..end]).unwrap(),
        }
    }

    pub fn inner(&self) -> &[u8] {
        &self.name
    }
}

impl core::fmt::Display for FieldName {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match core::str::from_utf8(&self.name) {
            Ok(s) => write!(f, "{}", s),
            Err(_) => write!(f, "<invalid UTF-8>"),
        }
    }
}

impl BinarySerializable for FieldName {
    fn serialize(&self, buf: &mut [u8]) -> Result<usize, crate::BinarySerializeError> {
        let name_len = self.name.len();
        if buf.len() < name_len + 1 {
            return Err(crate::BinarySerializeError::BufferTooSmall);
        }

        buf[..name_len].copy_from_slice(&self.name);
        buf[name_len] = 0; // Null terminator

        Ok(name_len + 1)
    }
}

impl BinaryDeserializable for FieldName {
    fn deserialize(
        buf: &[u8],
        _version: Version,
    ) -> Result<(usize, FieldName), BinaryDeserializeError> {
        let null_terminator_index = buf
            .iter()
            .position(|&b| b == 0)
            .ok_or(BinaryDeserializeError::UnterminatedFieldName)?;

        if null_terminator_index > FIELD_NAME_MAX_LEN {
            #[cfg(any(test, feature = "std"))]
            {
                use std::string::ToString;

                return Err(BinaryDeserializeError::FieldNameTooLong(
                    null_terminator_index,
                    std::string::String::from_utf8_lossy(&buf[..null_terminator_index]).to_string(),
                ));
            }
            #[cfg(not(any(test, feature = "std")))]
            {
                return Err(BinaryDeserializeError::FieldNameTooLong(
                    null_terminator_index,
                ));
            }
        }

        let mut name = heapless::Vec::<u8, FIELD_NAME_MAX_LEN>::new();
        for &byte in &buf[..null_terminator_index] {
            name.push(byte)
                .map_err(|_| BinaryDeserializeError::FieldNameStorageError)?;
        }

        Ok((null_terminator_index + 1, FieldName { name }))
    }
}

/// The definition of a value that is part of a packet.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ValueDef {
    /// The name of the value.
    pub name: FieldName,
    /// The kind of value that is being logged.
    pub kind: ValueKind,
    /// Number of this kind of value.
    pub count: u8,
}

impl Default for ValueDef {
    /// Note: Mostly implementing default for ArrayVec's sake.
    fn default() -> Self {
        Self {
            name: FieldName::default(),
            kind: ValueKind::default(),
            count: 1,
        }
    }
}

impl BinarySerializable for ValueDef {
    fn serialize(&self, buf: &mut [u8]) -> Result<usize, BinarySerializeError> {
        let name_size = self.name.serialize(buf)?;
        if buf.len() < name_size + 2 {
            return Err(BinarySerializeError::BufferTooSmall);
        }

        buf[name_size] = self.kind as u8;
        buf[name_size + 1] = self.count;

        Ok(name_size + 2)
    }
}

impl BinaryDeserializable for ValueDef {
    fn deserialize(
        buf: &[u8],
        version: Version,
    ) -> Result<(usize, ValueDef), BinaryDeserializeError> {
        let (name_size, name) = FieldName::deserialize(buf, version)?;

        if buf.len() < name_size + 2 {
            return Err(BinaryDeserializeError::BufferTooSmall);
        }

        let kind = ValueKind::try_from(buf[name_size])?;

        let count = buf[name_size + 1];

        Ok((name_size + 2, ValueDef { name, kind, count }))
    }
}

/// The kind of value that is being logged.
#[repr(u8)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ValueKind {
    #[default]
    U8 = 0,
    U16,
    U32,
    U64,
    I8,
    I16,
    I32,
    F32,
    Bool,
}

impl TryFrom<u8> for ValueKind {
    type Error = BinaryDeserializeError;

    fn try_from(value: u8) -> Result<Self, Self::Error> {
        match value {
            0 => Ok(ValueKind::U8),
            1 => Ok(ValueKind::U16),
            2 => Ok(ValueKind::U32),
            3 => Ok(ValueKind::U64),
            4 => Ok(ValueKind::I8),
            5 => Ok(ValueKind::I16),
            6 => Ok(ValueKind::I32),
            7 => Ok(ValueKind::F32),
            8 => Ok(ValueKind::Bool),
            _ => Err(BinaryDeserializeError::InvalidValueKind),
        }
    }
}
