use crate::Version;

use super::*;

#[test]
fn test_find_packet_def() {
    let mut metadata = Metadata::empty();
    let packet_def = PacketDef {
        id: 1,
        name: FieldName::from_str_truncating("pac"),
        log_to_terminal: true,
        values: heapless::Vec::new(),
    };
    metadata.packet_defs.push(packet_def.clone()).unwrap();

    assert_eq!(metadata.find_packet_def(1), Some(&packet_def));
    assert_eq!(metadata.find_packet_def(2), None);
}

#[test]
fn test_field_name_from_str() {
    let name = FieldName::from_str_truncating("test");
    assert_eq!(name.inner(), b"test");

    let long_name = FieldName::from_str_truncating("longer_than_8_bytes");
    assert_eq!(long_name.inner(), b"longer_t");
}

#[test]
fn test_field_name_serialize() {
    let name = FieldName::from_str_truncating("test");
    let mut buf = [0u8; 10];
    let size = name.serialize(&mut buf).unwrap();
    assert_eq!(size, 5);
    assert_eq!(&buf[..size], b"test\0");

    let long_name = FieldName::from_str_truncating("longer_than_8_bytes");
    let mut buf = [0u8; 10];
    let size = long_name.serialize(&mut buf).unwrap();
    assert_eq!(size, 9);
    assert_eq!(&buf[..size], b"longer_t\0");
}

const EX_VALUE_DEF: ValueDef = ValueDef {
    name: FieldName {
        name: heapless::Vec::from_array([b'v', b'a', b'l', b'u', b'e']),
    },
    kind: ValueKind::U16,
    count: 1,
};
const EX_VALUE_DEF_BYTES: [u8; 8] = [b'v', b'a', b'l', b'u', b'e', 0, ValueKind::U16 as u8, 1];

#[test]
fn test_value_def_serialize() {
    let mut buf = [0u8; 20];
    let size = EX_VALUE_DEF.serialize(&mut buf).unwrap();
    assert_eq!(size, 8);
    assert_eq!(&buf[..size], &EX_VALUE_DEF_BYTES);
}

#[test]
fn test_serialize_packet_def() {
    let pdef = PacketDef {
        id: 7,
        name: FieldName::from_str_truncating("pac"),
        log_to_terminal: true,
        values: heapless::Vec::from_slice(&[EX_VALUE_DEF.clone()]).unwrap(),
    };

    let mut buf = [0u8; 50];
    let size = pdef.serialize(&mut buf).unwrap();
    assert_eq!(size, 15);
    assert_eq!(buf[0], 7);
    assert_eq!(&buf[1..5], b"pac\0");
    assert_eq!(buf[5], 1); // log_to_terminal
    assert_eq!(buf[6], 1); // number of values
    assert_eq!(&buf[7..15], &EX_VALUE_DEF_BYTES);
}

#[test]
fn test_serialize_packet_def_buffer_too_small() {
    let pdef = PacketDef {
        id: 7,
        name: FieldName::from_str_truncating("pac"),
        log_to_terminal: true,
        values: heapless::Vec::from_slice(&[EX_VALUE_DEF.clone()]).unwrap(),
    };
    // Expected size = 1 + 4 (name + null) + 1 (log_to_terminal) + 1 (number of values) + 8 (value def) = 15

    // Using an empty buffer should return BufferTooSmall
    let mut buf = [0u8; 0];
    let result = pdef.serialize(&mut buf);
    assert!(matches!(result, Err(BinarySerializeError::BufferTooSmall)));

    // Too small for any name.
    let mut buf = [0u8; 1];
    let result = pdef.serialize(&mut buf);
    assert!(matches!(result, Err(BinarySerializeError::BufferTooSmall)));

    // Too small for full name
    let mut buf = [0u8; 3];
    let result = pdef.serialize(&mut buf);
    assert!(matches!(result, Err(BinarySerializeError::BufferTooSmall)));

    // Too small for count of values.
    let mut buf = [0u8; 5];
    let result = pdef.serialize(&mut buf);
    assert!(matches!(result, Err(BinarySerializeError::BufferTooSmall)));

    // Too small for all values.
    let mut buf = [0u8; 10];
    let result = pdef.serialize(&mut buf);
    assert!(matches!(result, Err(BinarySerializeError::BufferTooSmall)));

    // Now, exactly the right size should succeed.
    let mut buf = [0u8; 15];
    let result = pdef.serialize(&mut buf);
    assert!(matches!(result, Ok(15)));
}

#[test]
fn test_deserialize_field_name() {
    let buf = b"test\0";
    let name = FieldName::deserialize(buf, Version::LATEST).unwrap().1;
    assert_eq!(name.inner(), b"test");

    let buf = b"longer_t\0";
    let name = FieldName::deserialize(buf, Version::LATEST).unwrap().1;
    assert_eq!(name.inner(), b"longer_t");

    let buf = b"no_null_terminator";
    let result = FieldName::deserialize(buf, Version::LATEST);
    assert!(matches!(
        result,
        Err(BinaryDeserializeError::UnterminatedFieldName)
    ));

    let buf = b"too_long_name\0";
    let result = FieldName::deserialize(buf, Version::LATEST);
    assert!(matches!(
        result,
        Err(BinaryDeserializeError::FieldNameTooLong(13, _))
    ));
}

#[test]
fn test_deserialize_value_def() {
    let buf = &EX_VALUE_DEF_BYTES;
    let value_def = ValueDef::deserialize(buf, Version::LATEST).unwrap();
    assert_eq!(value_def, (buf.len(), EX_VALUE_DEF));
}

#[test]
fn test_deserialize_value_kind() {
    for i in 0..=0xFF_u8 {
        if let Ok(v) = ValueKind::try_from(i) {
            // in the ok path we make sure that valid values work both ways.
            assert_eq!(v as u8, i);
        }
    }

    assert_eq!(
        ValueKind::try_from(ValueKind::U8 as u8).unwrap(),
        ValueKind::U8
    );
}

#[test]
fn test_deserialize_packet_def() {
    let pdef = PacketDef {
        id: 7,
        name: FieldName::from_str_truncating("pac"),
        log_to_terminal: true,
        values: heapless::Vec::from_slice(&[EX_VALUE_DEF.clone()]).unwrap(),
    };

    let mut buf = [0u8; 50];
    let size = pdef.serialize(&mut buf).unwrap();
    assert_eq!(size, 15);

    let deserialized_pdef = PacketDef::deserialize(&buf[..size], Version::LATEST).unwrap();
    assert_eq!(deserialized_pdef, (size, pdef));
}

#[test]
fn test_deserialize_packet_def_v1() {
    let pdef = PacketDef {
        id: 7,
        name: FieldName::from_str_truncating("pac"),
        // The log to terminal flag should just default to false because it
        // never existed in the V1 format.
        log_to_terminal: false,
        values: heapless::Vec::from_slice(&[]).unwrap(),
    };

    // Manually encoded PacketDef to match because this is for V1 format which
    // no longer exists for serialization.
    let buf = [7, b'p', b'a', b'c', 0, 0];

    let deserialized_pdef = PacketDef::deserialize(&buf, Version::V1).unwrap();
    assert_eq!(deserialized_pdef, (buf.len(), pdef));
}

#[test]
fn test_metadata_isomorphic_serde() {
    let metadata = Metadata {
        packet_defs: heapless::Vec::from_slice(&[
            PacketDef {
                id: 1,
                name: FieldName::from_str_truncating("pac1"),
                log_to_terminal: true,
                values: heapless::Vec::from_slice(&[EX_VALUE_DEF.clone()]).unwrap(),
            },
            PacketDef {
                id: 2,
                name: FieldName::from_str_truncating("pac2"),
                log_to_terminal: false,
                values: heapless::Vec::from_slice(&[EX_VALUE_DEF.clone()]).unwrap(),
            },
        ])
        .unwrap(),
    };

    let mut buf = [0u8; 100];
    let size = metadata.serialize(&mut buf).unwrap();

    let deserialized_metadata =
        std::dbg!(Metadata::deserialize(&buf[..size], Version::LATEST).unwrap());

    assert_eq!(deserialized_metadata, (size, metadata));
}
