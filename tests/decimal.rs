//! Mainframe decimal vectors, boundaries and validation.

use jtc_utils::decimal::*;

#[test]
fn packed_variants_known_bytes() {
    assert_eq!(
        encode_packed(341, 4, PackedSign::Signed).unwrap(),
        [0, 0, 0x34, 0x1c]
    );
    assert_eq!(
        encode_packed(-341, 4, PackedSign::Signed).unwrap(),
        [0, 0, 0x34, 0x1d]
    );
    assert_eq!(
        encode_packed(341, 4, PackedSign::Unsigned).unwrap(),
        [0, 0, 0x34, 0x1f]
    );
    assert_eq!(
        encode_packed(12345678, 4, PackedSign::Signless).unwrap(),
        [0x12, 0x34, 0x56, 0x78]
    );
    for sign in [0xa, 0xc, 0xe, 0xf] {
        assert_eq!(
            decode_packed(&[0x12, 0x30 | sign], PackedSign::Signed).unwrap(),
            123
        );
    }
    for sign in [0xb, 0xd] {
        assert_eq!(
            decode_packed(&[0x12, 0x30 | sign], PackedSign::Signed).unwrap(),
            -123
        );
    }
    assert_eq!(
        decode_packed(&[0x1c], PackedSign::Unsigned),
        Err(Error::InvalidSign {
            index: 0,
            value: 0xc
        })
    );
    assert_eq!(
        encode_packed(-1, 2, PackedSign::Unsigned),
        Err(Error::NegativeUnsigned)
    );
    assert_eq!(
        encode_packed(1000, 2, PackedSign::Signed),
        Err(Error::Overflow)
    );
    assert_eq!(
        decode_packed(&[0xac], PackedSign::Signed),
        Err(Error::InvalidDigit {
            index: 0,
            value: 0xa
        })
    );
    assert_eq!(
        decode_packed(&[0x12], PackedSign::Signed),
        Err(Error::InvalidSign { index: 0, value: 2 })
    );
    assert_eq!(
        decode_packed(&[], PackedSign::Signed),
        Err(Error::InvalidLength)
    );
    assert_eq!(
        encode_packed(0, 0, PackedSign::Signed),
        Err(Error::InvalidLength)
    );
}

#[test]
fn zoned_variants_known_bytes() {
    assert_eq!(
        encode_zoned(-341, 4, Zone::Ebcdic, ZonedSign::Overpunch).unwrap(),
        [0xf0, 0xf3, 0xf4, 0xd1]
    );
    assert_eq!(
        encode_zoned(341, 4, Zone::Ebcdic, ZonedSign::Unsigned).unwrap(),
        [0xf0, 0xf3, 0xf4, 0xf1]
    );
    assert_eq!(
        encode_zoned(-341, 4, Zone::Ebcdic, ZonedSign::Leading).unwrap(),
        [0x60, 0xf3, 0xf4, 0xf1]
    );
    assert_eq!(
        encode_zoned(341, 4, Zone::Ebcdic, ZonedSign::Trailing).unwrap(),
        [0xf3, 0xf4, 0xf1, 0x4e]
    );
    assert_eq!(
        encode_zoned(-341, 4, Zone::Ascii, ZonedSign::Overpunch).unwrap(),
        [0x30, 0x33, 0x34, 0x71]
    );
    assert_eq!(
        encode_zoned(-341, 4, Zone::Ascii, ZonedSign::Leading).unwrap(),
        b"-341"
    );
    assert_eq!(
        decode_zoned(&[0x41, 0xc2], Zone::Ebcdic, ZonedSign::Overpunch),
        Err(Error::InvalidZone { index: 0, value: 4 })
    );
    assert_eq!(
        decode_zoned(&[0xf1, 0x92], Zone::Ebcdic, ZonedSign::Overpunch),
        Err(Error::InvalidSign { index: 1, value: 9 })
    );
    assert_eq!(
        decode_zoned(&[0xf1, 0xc2], Zone::Ebcdic, ZonedSign::Unsigned),
        Err(Error::InvalidZone {
            index: 1,
            value: 0xc
        })
    );
    assert_eq!(
        decode_zoned(b"x1", Zone::Ascii, ZonedSign::Leading),
        Err(Error::InvalidSign {
            index: 0,
            value: b'x'
        })
    );
    assert_eq!(
        decode_zoned(b"+", Zone::Ascii, ZonedSign::Leading),
        Err(Error::InvalidLength)
    );
    assert_eq!(
        encode_zoned(-1, 2, Zone::Ascii, ZonedSign::Unsigned),
        Err(Error::NegativeUnsigned)
    );
    assert_eq!(
        encode_zoned(123, 3, Zone::Ascii, ZonedSign::Trailing),
        Err(Error::Overflow)
    );
}

#[test]
fn exact_large_values_and_i128_limits() {
    let values = [
        i128::MIN,
        i128::MIN + 1,
        -9007199254740993,
        -341,
        -1,
        0,
        1,
        341,
        9007199254740993,
        i128::MAX,
    ];
    for value in values {
        let bytes = encode_packed(value, 20, PackedSign::Signed).unwrap();
        assert_eq!(decode_packed(&bytes, PackedSign::Signed).unwrap(), value);
        for zone in [Zone::Ascii, Zone::Ebcdic] {
            for sign in [
                ZonedSign::Overpunch,
                ZonedSign::Leading,
                ZonedSign::Trailing,
            ] {
                let bytes = encode_zoned(value, 40, zone, sign).unwrap();
                assert_eq!(decode_zoned(&bytes, zone, sign).unwrap(), value);
            }
        }
        if value >= 0 {
            for sign in [PackedSign::Unsigned, PackedSign::Signless] {
                let bytes = encode_packed(value, 20, sign).unwrap();
                assert_eq!(decode_packed(&bytes, sign).unwrap(), value);
            }
        }
    }
    let mut overflow = encode_packed(i128::MIN, 20, PackedSign::Signed).unwrap();
    *overflow.last_mut().unwrap() = 0x8c; // |MIN| is one greater than MAX
    assert_eq!(
        decode_packed(&overflow, PackedSign::Signed),
        Err(Error::Overflow)
    );
    assert_eq!(
        decode_packed(&[0x99; 40], PackedSign::Signless),
        Err(Error::Overflow)
    );
}
