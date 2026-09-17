/// The value equivalent of the [`crate::metadata::ValueKind`] enum.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum PacketValue {
    U8(u8),
    U16(u16),
    U32(u32),
    U64(u64),
    I8(i8),
    I16(i16),
    I32(i32),
    F32(f32),
    Bool(bool),
}

impl PacketValue {
    /// Get an f64 requivalent of the value.
    ///
    /// Care should be taken when the value is a u64, as values over 2^53 may
    /// become non-exact.
    ///
    /// This was originally included because Rerun (the data visualization app),
    /// represents most values as f64.
    pub fn as_f64(&self) -> f64 {
        match self {
            PacketValue::U8(v) => *v as f64,
            PacketValue::U16(v) => *v as f64,
            PacketValue::U32(v) => *v as f64,
            PacketValue::U64(v) => *v as f64,
            PacketValue::I8(v) => *v as f64,
            PacketValue::I16(v) => *v as f64,
            PacketValue::I32(v) => *v as f64,
            PacketValue::F32(v) => *v as f64,
            PacketValue::Bool(v) => (*v as u8) as f64,
        }
    }
}
