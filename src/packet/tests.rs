use crate::metadata::FieldName;

use super::*;

/// The answer to the life, the universe, and everything... sort of.
const TEST_PACKET_ID: u8 = 0x42;

#[test]
fn test_new_writes_id() {
    let mut buf = [0u8; 20];

    let w = PacketWriter::new(&mut buf, PacketHeader::new(TEST_PACKET_ID, 0x1234)).unwrap();

    assert_eq!(w.space(), 17);
    assert_eq!(buf[0], TEST_PACKET_ID);
    assert_eq!(buf[1..3], 0x1234u16.to_le_bytes());
}

#[test]
fn test_writing_u8() {
    let mut buf = [0u8; 16];

    let mut w = PacketWriter::new(&mut buf, PacketHeader::new(TEST_PACKET_ID, 0x1234)).unwrap();

    assert_eq!(w.space(), 13);
    w.write_u8(0xAB).unwrap();
    assert_eq!(w.space(), 12);
    assert_eq!(buf[3], 0xAB);
}

#[test]
fn test_write_bool_single() {
    let mut buf = [0u8; 16];

    let mut w = PacketWriter::new(&mut buf, PacketHeader::new(TEST_PACKET_ID, 0x1234)).unwrap();

    w.write_bool(true).unwrap();
    assert_eq!(
        w.space(),
        buf.len() - PACKET_HEADER_SIZE - 1,
        "byte should be claimed immediately for partial"
    );
    assert_eq!(
        buf[PACKET_HEADER_SIZE], 0b0000_0001,
        "should write the bits in place with zero padding"
    );
}

#[test]
fn test_write_bool_multiple_less_than_8() {
    let mut buf = [0u8; 16];

    let mut w = PacketWriter::new(&mut buf, PacketHeader::new(TEST_PACKET_ID, 0x1234)).unwrap();

    w.write_bool(true).unwrap();
    w.write_bool(false).unwrap();
    w.write_bool(true).unwrap();
    assert_eq!(
        w.space(),
        buf.len() - PACKET_HEADER_SIZE - 1,
        "only one byte should be claimed for 3 bits"
    );
    assert_eq!(buf[PACKET_HEADER_SIZE], 0b0000_0101);
}

#[test]
fn test_write_bool_full_byte() {
    let mut buf = [0u8; 16];

    let mut w = PacketWriter::new(&mut buf, PacketHeader::new(TEST_PACKET_ID, 0x1234)).unwrap();

    for _ in 0..4 {
        w.write_bool(true).unwrap();
        w.write_bool(false).unwrap();
    }
    assert!(
        !w.has_sub_byte_values(),
        "should not have any partials have byte aligned number of bits"
    );
    assert_eq!(
        w.space(),
        buf.len() - PACKET_HEADER_SIZE - 1,
        "only one byte should be claimed for 8 bits"
    );
    assert_eq!(buf[PACKET_HEADER_SIZE], 0b1010_1010);
}

#[test]
fn test_write_bool_full_byte_overflow() {
    let mut buf = [0u8; 16];

    let mut w = PacketWriter::new(&mut buf, PacketHeader::new(TEST_PACKET_ID, 0x1234)).unwrap();

    for _ in 0..4 {
        w.write_bool(true).unwrap();
        w.write_bool(false).unwrap();
    }
    w.write_bool(true).unwrap();

    assert_eq!(
        w.space(),
        buf.len() - PACKET_HEADER_SIZE - 2,
        "overflowing bit writing should claim another byte"
    );
    assert_eq!(buf[PACKET_HEADER_SIZE], 0b1010_1010);
    assert_eq!(buf[PACKET_HEADER_SIZE + 1], 0b0000_0001);
}

#[test]
fn test_write_bool_zeros_byte() {
    let mut buf = [0xFF; 16];

    let mut w = PacketWriter::new(&mut buf, PacketHeader::new(TEST_PACKET_ID, 0x1234)).unwrap();

    w.write_bool(true).unwrap();
    assert_eq!(buf[PACKET_HEADER_SIZE], 0b0000_0001);
}

#[test]
fn test_write_partial_followed_by_full() {
    let mut buf = [0u8; 16];

    let mut w = PacketWriter::new(&mut buf, PacketHeader::new(TEST_PACKET_ID, 0x1234)).unwrap();

    w.write_bool(true).unwrap();
    w.write_u8(0xAB).unwrap();
    w.write_bool(false).unwrap();
    w.write_bool(true).unwrap();
    w.write_u32(0x5AFE_BEEF).unwrap();

    assert_eq!(w.space(), buf.len() - PACKET_HEADER_SIZE - 7);
    assert_eq!(buf[PACKET_HEADER_SIZE], 0b0000_0001);
    assert_eq!(buf[PACKET_HEADER_SIZE + 1], 0xAB);
    assert_eq!(buf[PACKET_HEADER_SIZE + 2], 0b0000_0001);
    assert_eq!(
        buf[(PACKET_HEADER_SIZE + 3)..(PACKET_HEADER_SIZE + 7)],
        0x5AFE_BEEF_u32.to_le_bytes()
    );
}

#[test]
fn test_initialize_with_zero_length_buffer_returns_none() {
    let mut buf = [0u8; 0];

    let w = PacketWriter::new(&mut buf, PacketHeader::new(TEST_PACKET_ID, 0x1234));
    assert!(w.is_none());
}

#[test]
fn test_writing_slice() {
    let mut buf = [0u8; 16];

    let mut w = PacketWriter::new(&mut buf, PacketHeader::new(TEST_PACKET_ID, 0x1234)).unwrap();
    let data = [0xAB, 0xCD, 0xEF];

    w.write_slice(&data).unwrap();
    assert_eq!(w.space(), 10);

    assert_eq!(buf[3..6], data);
}

#[test]
fn test_writing_slice_off_end() {
    let mut buf = [0u8; 7];

    let mut w = PacketWriter::new(&mut buf, PacketHeader::new(TEST_PACKET_ID, 0x1234)).unwrap();
    let data = [0xAB, 0xCD, 0xEF];
    w.write_slice(&data).unwrap();

    let more_data = [0x01, 0x02, 0x03, 0x04, 0x05, 0x06, 0x07, 0x08, 0x09, 0x0A];
    assert!(
        w.write_slice(&more_data).is_err(),
        "should not allow writing more data than available space"
    );

    assert_eq!(w.space(), 1, "should not use any space when write fails");
}

#[test]
fn test_write_read_all() {
    let mut buf = [0u8; 64];

    let mut w = PacketWriter::new(&mut buf, PacketHeader::new(TEST_PACKET_ID, 0x1234)).unwrap();
    w.write_u32(0x12345678).unwrap();
    w.write_u64(0x1234567890ABCDEF).unwrap();
    w.write_f32(3.14159).unwrap();

    let (mut r, header) = PacketValueReader::new(&buf).unwrap();
    assert_eq!(header.id, TEST_PACKET_ID);
    assert_eq!(header.timestamp, 0x1234);
    assert_eq!(r.read_u32().unwrap(), 0x12345678);
    assert_eq!(r.read_u64().unwrap(), 0x1234567890ABCDEF);
    assert!((r.read_f32().unwrap() - 3.14159).abs() < f32::EPSILON);
}

pub const TEST_PACKET_DEF: PacketDef = PacketDef {
    id: TEST_PACKET_ID,
    name: FieldName::from_array(*b"test"),
    log_to_terminal: false,
    values: heapless::Vec::from_array([
        ValueDef {
            name: FieldName::from_array(*b"a"),
            kind: ValueKind::U8,
            count: 1,
        },
        ValueDef {
            name: FieldName::from_array(*b"b"),
            kind: ValueKind::U16,
            count: 1,
        },
        ValueDef {
            name: FieldName::from_array(*b"c"),
            kind: ValueKind::F32,
            count: 1,
        },
    ]),
};

const TEST_PACKET_LIST: [PacketDef; 1] = [TEST_PACKET_DEF];
const TEST_PACKET_LIST_SLICE: &'static [PacketDef] = &TEST_PACKET_LIST;

#[test]
fn test_read_values_all_whole() {
    let mut buf = [0u8; 64];

    let mut w = PacketWriter::new(&mut buf, PacketHeader::new(TEST_PACKET_ID, 0x1234)).unwrap();
    w.write_u8(0x12).unwrap();
    w.write_u16(0x3456).unwrap();
    w.write_f32(3.1415926).unwrap();

    let (r, header) = PacketValueReader::new(&buf).unwrap();

    let mut iterator = r.read_values(TEST_PACKET_LIST_SLICE).unwrap();

    assert_eq!(header.id, TEST_PACKET_ID);
    assert_eq!(header.timestamp, 0x1234);
    assert_eq!(iterator.next(), Some(Ok(PacketValue::U8(0x12))));
    assert_eq!(iterator.next(), Some(Ok(PacketValue::U16(0x3456))));
    assert_eq!(iterator.next(), Some(Ok(PacketValue::F32(3.1415926))));
    assert_eq!(iterator.next(), None);
}

#[test]
fn test_read_values_partial() {
    let mut buf = [0u8; 64];

    let mut w = PacketWriter::new(&mut buf, PacketHeader::new(TEST_PACKET_ID, 0x1234)).unwrap();
    w.write_bool(true).unwrap();
    w.write_bool(true).unwrap();
    w.write_bool(false).unwrap();

    let packet_def = PacketDef {
        id: TEST_PACKET_ID,
        name: FieldName::from_str_truncating("s"),
        log_to_terminal: false,
        values: heapless::Vec::from_array([
            ValueDef {
                name: FieldName::from_array(*b"a"),
                kind: ValueKind::Bool,
                count: 1,
            },
            ValueDef {
                name: FieldName::from_array(*b"b"),
                kind: ValueKind::Bool,
                count: 1,
            },
            ValueDef {
                name: FieldName::from_array(*b"c"),
                kind: ValueKind::Bool,
                count: 1,
            },
        ]),
    };
    let slice = &[packet_def];

    let (r, header) = PacketValueReader::new(&buf).unwrap();
    let mut iterator = r.read_values(slice).unwrap();

    assert_eq!(header.id, TEST_PACKET_ID);
    assert_eq!(header.timestamp, 0x1234);
    assert_eq!(iterator.next(), Some(Ok(PacketValue::Bool(true))));
    assert_eq!(iterator.next(), Some(Ok(PacketValue::Bool(true))));
    assert_eq!(iterator.next(), Some(Ok(PacketValue::Bool(false))));
    assert_eq!(iterator.next(), None);
}

#[test]
fn test_read_values_partial_multiple() {
    let mut buf = [0u8; 64];

    let mut w = PacketWriter::new(&mut buf, PacketHeader::new(TEST_PACKET_ID, 0x1234)).unwrap();
    w.write_bool(true).unwrap();
    w.write_bool(true).unwrap();
    w.write_bool(false).unwrap();
    w.write_bool(true).unwrap();
    w.write_bool(false).unwrap();

    let packet_def = PacketDef {
        id: TEST_PACKET_ID,
        name: FieldName::from_str_truncating("s"),
        log_to_terminal: false,
        values: heapless::Vec::from_array([
            ValueDef {
                name: FieldName::from_array(*b"a"),
                kind: ValueKind::Bool,
                count: 3,
            },
            ValueDef {
                name: FieldName::from_array(*b"b"),
                kind: ValueKind::Bool,
                count: 1,
            },
            ValueDef {
                name: FieldName::from_array(*b"c"),
                kind: ValueKind::Bool,
                count: 1,
            },
        ]),
    };
    let slice = &[packet_def];

    let (r, header) = PacketValueReader::new(&buf).unwrap();
    let mut iterator = r.read_values(slice).unwrap();

    assert_eq!(header.id, TEST_PACKET_ID);
    assert_eq!(header.timestamp, 0x1234);
    assert_eq!(iterator.next(), Some(Ok(PacketValue::Bool(true))));
    assert_eq!(iterator.next(), Some(Ok(PacketValue::Bool(true))));
    assert_eq!(iterator.next(), Some(Ok(PacketValue::Bool(false))));
    assert_eq!(iterator.next(), Some(Ok(PacketValue::Bool(true))));
    assert_eq!(iterator.next(), Some(Ok(PacketValue::Bool(false))));
    assert_eq!(iterator.next(), None);
}

#[test]
fn test_read_values_partial_many_bytes() {
    let mut buf = [0u8; 64];

    let mut w = PacketWriter::new(&mut buf, PacketHeader::new(TEST_PACKET_ID, 0x1234)).unwrap();
    w.write_bool(true).unwrap();
    w.write_bool(true).unwrap();
    w.write_bool(false).unwrap();
    w.write_bool(true).unwrap();
    w.write_bool(true).unwrap();
    w.write_bool(false).unwrap();
    w.write_bool(true).unwrap();
    w.write_bool(true).unwrap();
    w.write_bool(false).unwrap();
    w.write_bool(true).unwrap();
    w.write_bool(false).unwrap();

    let packet_def = PacketDef {
        id: TEST_PACKET_ID,
        name: FieldName::from_str_truncating("s"),
        log_to_terminal: false,
        values: heapless::Vec::from_array([
            ValueDef {
                name: FieldName::from_array(*b"a"),
                kind: ValueKind::Bool,
                count: 9,
            },
            ValueDef {
                name: FieldName::from_array(*b"b"),
                kind: ValueKind::Bool,
                count: 1,
            },
            ValueDef {
                name: FieldName::from_array(*b"c"),
                kind: ValueKind::Bool,
                count: 1,
            },
        ]),
    };
    let slice = &[packet_def];

    let (r, header) = PacketValueReader::new(&buf).unwrap();
    let mut iterator = r.read_values(slice).unwrap();

    assert_eq!(header.id, TEST_PACKET_ID);
    assert_eq!(header.timestamp, 0x1234);
    assert_eq!(iterator.next(), Some(Ok(PacketValue::Bool(true))));
    assert_eq!(iterator.next(), Some(Ok(PacketValue::Bool(true))));
    assert_eq!(iterator.next(), Some(Ok(PacketValue::Bool(false))));
    assert_eq!(iterator.next(), Some(Ok(PacketValue::Bool(true))));
    assert_eq!(iterator.next(), Some(Ok(PacketValue::Bool(true))));
    assert_eq!(iterator.next(), Some(Ok(PacketValue::Bool(false))));
    assert_eq!(iterator.next(), Some(Ok(PacketValue::Bool(true))));
    assert_eq!(iterator.next(), Some(Ok(PacketValue::Bool(true))));
    assert_eq!(iterator.next(), Some(Ok(PacketValue::Bool(false))));
    assert_eq!(iterator.next(), Some(Ok(PacketValue::Bool(true))));
    assert_eq!(iterator.next(), Some(Ok(PacketValue::Bool(false))));
    assert_eq!(iterator.next(), None);
}

// TODO: test with multi-count value def
// TODO: test with multi-count partials
