//! Module for CRC32 checksum computation.
//!
//! This module contains the [`Crc32Interface`] trait, which defines some
//! interface for computing CRC32 checksums.
//!
//! Most of the time, the embedded firmware will have a hardware CRC32
//! peripheral that can be used and should implement the interface. On the
//! desktop where performance is less critical, the software implementation is
//! used.

#[cfg(test)]
mod tests;

/// The polynomial used for CRC32 computation.
///
/// This is essentially just the standard polynomial used in many places such
/// as ethernet, gzip, png, mpeg-2, and more.
pub const CRC32_POLYNOMIAL: u32 = 0x04C11DB7;
/// The initial value to start the calculation with.
pub const CRC32_INITIAL: u32 = 0xFFFFFFFF;
/// The value to XOR with the final CRC32 result.
pub const CRC32_FINAL_XOR: u32 = 0xFFFFFFFF;
/// Whether to reflect the input bytes.
pub const CRC32_REFLECT_INPUT: bool = true;
/// Whether to reflect the output CRC32 value.
pub const CRC32_REFLECT_OUTPUT: bool = true;
/// This is the CRC32 checksum of the ASCII string "123456789" using the above
/// parameters. It is used to verify that the implementation is correct.
pub const CRC32_CHECK: u32 = 0xCBF43926;
/// This is the CRC32 residue.
///
/// In other words, if you compute the CRC32 of a message, including the CRC32
/// checksum itself, then you will get this.
pub const CRC32_RESIDUE: u32 = 0xDEBB20E3;

const SOFTWARE_CRC32_ALG: crc::Algorithm<u32> = crc::Algorithm {
    width: 32,
    poly: CRC32_POLYNOMIAL,
    init: CRC32_INITIAL,
    refin: CRC32_REFLECT_INPUT,
    refout: CRC32_REFLECT_OUTPUT,
    xorout: CRC32_FINAL_XOR,
    check: CRC32_CHECK,
    residue: CRC32_RESIDUE,
};

/// Generic interface for some CRC32 calculator.
pub trait Crc32Interface {
    /// Computes the CRC32 checksum of the given data.
    ///
    /// # Params
    ///
    /// * `data`: the data to compute the checksum for
    fn crc32(&mut self, data: &[u8]) -> u32;

    /// Validates the CRC32 checksum of the given data.
    ///
    /// This assumes that the last 4 bytes of the data are the CRC32 checksum.
    /// Will return false if the data is less than 4 bytes long.
    fn validate_crc32(&mut self, data: &[u8]) -> bool {
        let Some((body, crc_bytes)) = data.split_last_chunk::<4>() else {
            return false;
        };

        self.crc32(body) == u32::from_le_bytes(*crc_bytes)
    }
}

/// Software implementation of the [`Crc32Interface`] trait.
///
/// This is primarily used on desktop applications, or where there is no access
/// to a hardware peripheral.
pub struct SoftwareCrc32 {
    crc: crc::Crc<u32>,
}

impl SoftwareCrc32 {
    /// Create a new software crc implementation with the correct parameters
    /// for the CRC used in the file format.
    pub fn new() -> Self {
        Self {
            crc: crc::Crc::<u32>::new(&SOFTWARE_CRC32_ALG),
        }
    }
}

impl Default for SoftwareCrc32 {
    fn default() -> Self {
        Self::new()
    }
}

impl Crc32Interface for SoftwareCrc32 {
    fn crc32(&mut self, data: &[u8]) -> u32 {
        self.crc.checksum(data)
    }
}
