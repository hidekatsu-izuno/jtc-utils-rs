//! Mainframe packed and zoned decimal storage, using exact `i128` coefficients.
//! An implied decimal scale belongs to the record schema/application; these
//! functions do not round or convert through floating point.

use std::{error, fmt};

/// Sign representation in packed decimal (BCD).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PackedSign {
    /// A final sign nibble; emit C/D, accept A/C/E/F positive and B/D negative.
    Signed,
    /// A final F nibble, accepting nonnegative values only.
    Unsigned,
    /// No sign nibble: every nibble is a digit, accepting nonnegative values only.
    Signless,
}

/// Zone nibbles and separate sign bytes in zoned decimal.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Zone {
    /// Digit zone 3; overpunch zone 3 positive / 7 negative.
    Ascii,
    /// Digit zone F; overpunch emits C/D, accepts A/C/E/F and B/D.
    Ebcdic,
}

/// Sign storage in zoned decimal.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ZonedSign {
    /// The high nibble of the final digit contains the sign.
    Overpunch,
    /// Digits only, with no sign.
    Unsigned,
    /// A separate leading `+` / `-` byte in the selected zone's encoding.
    Leading,
    /// A separate trailing `+` / `-` byte in the selected zone's encoding.
    Trailing,
}

/// Invalid decimal storage or a value outside the requested capacity.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Error {
    /// A field needs at least one digit (and space for a separate sign).
    InvalidLength,
    /// The value exceeds the field or `i128` capacity.
    Overflow,
    /// A negative value was supplied to an unsigned representation.
    NegativeUnsigned,
    /// A digit nibble was not in 0..=9.
    InvalidDigit {
        /// Byte offset within the field.
        index: usize,
        /// Invalid nibble value.
        value: u8,
    },
    /// A sign nibble/byte was invalid.
    InvalidSign {
        /// Byte offset within the field.
        index: usize,
        /// Invalid sign nibble or byte.
        value: u8,
    },
    /// A digit's zone nibble did not match the chosen encoding.
    InvalidZone {
        /// Byte offset within the field.
        index: usize,
        /// Invalid high nibble.
        value: u8,
    },
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidLength => f.write_str("decimal field is too short"),
            Self::Overflow => f.write_str("decimal value exceeds field or i128 capacity"),
            Self::NegativeUnsigned => f.write_str("unsigned decimal cannot store a negative value"),
            Self::InvalidDigit { index, value } => {
                write!(f, "invalid decimal digit {value:X} at byte {index}")
            }
            Self::InvalidSign { index, value } => {
                write!(f, "invalid decimal sign {value:X} at byte {index}")
            }
            Self::InvalidZone { index, value } => {
                write!(f, "invalid decimal zone {value:X} at byte {index}")
            }
        }
    }
}
impl error::Error for Error {}

fn digit(value: &mut u128, nibble: u8, index: usize) -> Result<(), Error> {
    if nibble > 9 {
        return Err(Error::InvalidDigit {
            index,
            value: nibble,
        });
    }
    *value = value
        .checked_mul(10)
        .and_then(|v| v.checked_add(u128::from(nibble)))
        .ok_or(Error::Overflow)?;
    Ok(())
}

fn signed(value: u128, negative: bool) -> Result<i128, Error> {
    if negative && value == (1u128 << 127) {
        return Ok(i128::MIN);
    }
    let value = i128::try_from(value).map_err(|_| Error::Overflow)?;
    Ok(if negative { -value } else { value })
}

/// Decode BCD bytes, validating every digit and the final sign nibble.
pub fn decode_packed(input: &[u8], sign: PackedSign) -> Result<i128, Error> {
    if input.is_empty() {
        return Err(Error::InvalidLength);
    }
    let mut value = 0;
    let mut negative = false;
    for (index, &byte) in input.iter().enumerate() {
        digit(&mut value, byte >> 4, index)?;
        let low = byte & 15;
        if index == input.len() - 1 && sign != PackedSign::Signless {
            negative = match (sign, low) {
                (PackedSign::Unsigned, 0xf) => false,
                (PackedSign::Signed, 0xa | 0xc | 0xe | 0xf) => false,
                (PackedSign::Signed, 0xb | 0xd) => true,
                _ => return Err(Error::InvalidSign { index, value: low }),
            };
        } else {
            digit(&mut value, low, index)?;
        }
    }
    signed(value, negative)
}

/// Encode BCD bytes with leading zeroes and checked capacity.
pub fn encode_packed(value: i128, width: usize, sign: PackedSign) -> Result<Vec<u8>, Error> {
    if width == 0 {
        return Err(Error::InvalidLength);
    }
    if value < 0 && sign != PackedSign::Signed {
        return Err(Error::NegativeUnsigned);
    }
    let capacity =
        width.checked_mul(2).ok_or(Error::Overflow)? - usize::from(sign != PackedSign::Signless);
    let digits = value.unsigned_abs().to_string();
    if digits.len() > capacity {
        return Err(Error::Overflow);
    }
    let mut output = vec![0; width];
    for (offset, digit) in digits.bytes().enumerate() {
        let index = capacity - digits.len() + offset;
        output[index / 2] |= (digit - b'0') << if index % 2 == 0 { 4 } else { 0 };
    }
    if sign != PackedSign::Signless {
        output[width - 1] |= if sign == PackedSign::Unsigned {
            0xf
        } else if value < 0 {
            0xd
        } else {
            0xc
        };
    }
    Ok(output)
}

fn zone_nibble(zone: Zone) -> u8 {
    match zone {
        Zone::Ascii => 3,
        Zone::Ebcdic => 15,
    }
}
fn sign_byte(zone: Zone, negative: bool) -> u8 {
    match (zone, negative) {
        (Zone::Ascii, false) => b'+',
        (Zone::Ascii, true) => b'-',
        (Zone::Ebcdic, false) => 0x4e,
        (Zone::Ebcdic, true) => 0x60,
    }
}

/// Decode zoned bytes, validating both zone and digit nibbles.
pub fn decode_zoned(input: &[u8], zone: Zone, sign: ZonedSign) -> Result<i128, Error> {
    let separate = matches!(sign, ZonedSign::Leading | ZonedSign::Trailing);
    if input.len() <= usize::from(separate) {
        return Err(Error::InvalidLength);
    }
    let mut negative = false;
    let (start, end) = match sign {
        ZonedSign::Leading => (1, input.len()),
        ZonedSign::Trailing => (0, input.len() - 1),
        _ => (0, input.len()),
    };
    if separate {
        let index = if sign == ZonedSign::Leading {
            0
        } else {
            input.len() - 1
        };
        negative = if input[index] == sign_byte(zone, false) {
            false
        } else if input[index] == sign_byte(zone, true) {
            true
        } else {
            return Err(Error::InvalidSign {
                index,
                value: input[index],
            });
        };
    }
    let mut value = 0;
    for (index, &byte) in input.iter().enumerate().take(end).skip(start) {
        let high = byte >> 4;
        if sign == ZonedSign::Overpunch && index == end - 1 {
            negative = match (zone, high) {
                (Zone::Ascii, 3) | (Zone::Ebcdic, 0xa | 0xc | 0xe | 0xf) => false,
                (Zone::Ascii, 7) | (Zone::Ebcdic, 0xb | 0xd) => true,
                _ => return Err(Error::InvalidSign { index, value: high }),
            };
        } else if high != zone_nibble(zone) {
            return Err(Error::InvalidZone { index, value: high });
        }
        digit(&mut value, byte & 15, index)?;
    }
    signed(value, negative)
}

/// Encode zoned bytes with leading zeroes and checked capacity.
pub fn encode_zoned(
    value: i128,
    width: usize,
    zone: Zone,
    sign: ZonedSign,
) -> Result<Vec<u8>, Error> {
    let separate = matches!(sign, ZonedSign::Leading | ZonedSign::Trailing);
    if width <= usize::from(separate) {
        return Err(Error::InvalidLength);
    }
    if value < 0 && sign == ZonedSign::Unsigned {
        return Err(Error::NegativeUnsigned);
    }
    let capacity = width - usize::from(separate);
    let digits = value.unsigned_abs().to_string();
    if digits.len() > capacity {
        return Err(Error::Overflow);
    }
    let mut output = vec![zone_nibble(zone) << 4; width];
    let start = usize::from(sign == ZonedSign::Leading);
    for (index, digit) in digits.bytes().enumerate() {
        output[start + capacity - digits.len() + index] |= digit - b'0';
    }
    if separate {
        let index = if sign == ZonedSign::Leading {
            0
        } else {
            width - 1
        };
        output[index] = sign_byte(zone, value < 0);
    } else if sign == ZonedSign::Overpunch {
        let high = match (zone, value < 0) {
            (Zone::Ascii, false) => 3,
            (Zone::Ascii, true) => 7,
            (Zone::Ebcdic, false) => 0xc,
            (Zone::Ebcdic, true) => 0xd,
        };
        output[width - 1] = (high << 4) | (output[width - 1] & 15);
    }
    Ok(output)
}
