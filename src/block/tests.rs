use crate::crc32::{Crc32Interface, SoftwareCrc32};
use crate::metadata::{FieldName, PacketDef, ValueDef, ValueKind};

use super::*;

#[test]
fn test_block_header_serialize_size() {
    let mut buf = [0u8; BLOCK_SIZE];

    let header = BlockHeader {
        version: Version::LATEST,
        block_type: BlockType::Data,
        size: 123,
        block_index: 7,
        timestamp: 0x1234_5678_9abc_def0,
        header_crc: 0,
    };

    let size = header
        .serialize(&mut buf, &mut SoftwareCrc32::new())
        .expect("Writing to block buffer should not fail");

    assert_eq!(size, HEADER_SIZE as usize);
}

#[test]
fn test_block_header_isomorphic() {
    let mut buf = [0u8; BLOCK_SIZE];

    let header = BlockHeader {
        version: Version::LATEST,
        block_type: BlockType::Data,
        size: 123,
        block_index: 7,
        timestamp: 0x1234_5678_9abc_def0,
        header_crc: 0,
    };

    let size = header
        .serialize(&mut buf, &mut SoftwareCrc32::new())
        .expect("Writing to block buffer should not fail");

    assert_eq!(size, HEADER_SIZE as usize);

    let (de_size, deserialized_header) = BlockHeader::deserialize(&buf[..size], Version::LATEST)
        .expect("Deserializing from block buffer should not fail");

    assert_eq!(de_size, size);

    // Bit of a hack, but I don't really care if the CRC is correct in this
    // particular part of the test.
    let header = {
        let mut header = header.clone();
        header.header_crc = deserialized_header.header_crc;
        header
    };
    assert_eq!(header, deserialized_header);
}

#[test]
fn test_write_empty_data_block() {
    let header = BlockHeader {
        version: Version::LATEST,
        block_type: BlockType::Data,
        size: 0,
        block_index: 0,
        timestamp: 0x1234_5678_9abc_def0,
        header_crc: 0,
    };

    let mut crc_interface = SoftwareCrc32::new();

    let mut buf = [0u8; BLOCK_SIZE];
    let writer = BlockWriter::new(&mut buf, header);
    writer.finish(&mut crc_interface);

    // Verify CRC
    let calculated_crc = crc_interface.crc32(&buf[HEADER_SIZE as usize..HEADER_SIZE as usize]);
    let written_crc = u32::from_le_bytes([
        buf[HEADER_SIZE as usize],
        buf[HEADER_SIZE as usize + 1],
        buf[HEADER_SIZE as usize + 2],
        buf[HEADER_SIZE as usize + 3],
    ]);

    assert_eq!(calculated_crc, written_crc);
}

#[test]
fn test_block_writer_leaves_space_for_crc() {
    let header = BlockHeader {
        version: Version::LATEST,
        block_type: BlockType::Data,
        size: 0,
        block_index: 0,
        timestamp: 0x1234_5678_9abc_def0,
        header_crc: 0,
    };

    let mut buf = [0u8; BLOCK_SIZE];
    let mut writer = BlockWriter::new(&mut buf, header);

    // The space should be BLOCK_SIZE - 4 for the CRC
    let space = writer.space();
    assert_eq!(space, BLOCK_SIZE - HEADER_SIZE as usize - 4);

    for i in 0..space {
        writer
            .write(&[(i % 256) as u8])
            .expect("Writing to block should not fail");
    }

    assert_eq!(writer.space(), 0);
    assert!(
        writer.write(&[0u8]).is_err(),
        "Writing beyond block space should fail"
    );
}

const ACCEL_PACKET_ID: u8 = 0x01;
pub const ACCEL_PACKET_DEF: PacketDef = PacketDef {
    id: ACCEL_PACKET_ID,
    name: FieldName::from_array(*b"accel"),
    log_to_terminal: false,
    values: heapless::Vec::from_array([ValueDef {
        name: FieldName::from_array(*b"xyz"),
        kind: ValueKind::F32,
        count: 3,
    }]),
};

pub const FAKE_META: Metadata = Metadata {
    packet_defs: heapless::Vec::from_array([ACCEL_PACKET_DEF]),
};

#[test]
fn test_block_writer_write_meta_block_basic() {
    let mut buf = [0u8; BLOCK_SIZE];
    let expected_header = BlockHeader {
        version: Version::LATEST,
        block_type: BlockType::Meta,
        size: 0,
        block_index: 0,
        timestamp: 0x1234_5678_9abc_def0,
        header_crc: 0,
    };

    let mut writer = BlockWriter::new(&mut buf, expected_header.clone());

    writer
        .write_meta_block(&FAKE_META)
        .expect("Writing metadata block should not fail");

    let _ = writer.finish(&mut SoftwareCrc32::new());

    let (_de_size, deserialized_header) =
        BlockHeader::deserialize(&buf[..HEADER_SIZE as usize], Version::LATEST)
            .expect("Deserializing from block buffer should not fail");

    assert_eq!(deserialized_header.version, expected_header.version);
    assert_eq!(deserialized_header.block_type, expected_header.block_type);
    assert_eq!(deserialized_header.timestamp, expected_header.timestamp);
}
