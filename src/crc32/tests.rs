use super::*;

#[test]
fn test_valid_software_crc_params() {
    let mut sw_crc = SoftwareCrc32::new();
    assert_eq!(sw_crc.crc32(b"123456789"), 0xCBF4_3926);
}
