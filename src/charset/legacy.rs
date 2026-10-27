use super::{Charset, EncodeOptions, Error, ErrorPolicy};
use crate::table::{self, Table};

fn tables(charset: Charset) -> (Table, Table) {
    use Charset::*;
    match charset {
        Jef => (table::ENCODE_X_FUJITSU_JEF, table::DECODE_X_FUJITSU_JEF),
        FujitsuEbcdicAscii => (
            table::ENCODE_X_FUJITSU_EBCDIC_ASCII,
            table::DECODE_X_FUJITSU_EBCDIC_ASCII,
        ),
        FujitsuEbcdicKana => (
            table::ENCODE_X_FUJITSU_EBCDIC_KANA,
            table::DECODE_X_FUJITSU_EBCDIC_KANA,
        ),
        FujitsuEbcdicLower => (
            table::ENCODE_X_FUJITSU_EBCDIC_LOWER,
            table::DECODE_X_FUJITSU_EBCDIC_LOWER,
        ),
        Cp930 => (table::ENCODE_CP8482, table::DECODE_CP8482),
        Cp939 => (table::ENCODE_CP5123, table::DECODE_CP5123),
        Cp942 => (table::ENCODE_CP942, table::DECODE_CP942),
        Cp942C => (table::ENCODE_CP942C, table::DECODE_CP942C),
        Cp943C => (table::ENCODE_CP943C, table::DECODE_CP943C),
        Standard(_) => unreachable!(),
    }
}

fn is_mixed(charset: Charset) -> bool {
    matches!(charset, Charset::Cp930 | Charset::Cp939)
}
fn is_sjis(charset: Charset) -> bool {
    matches!(charset, Charset::Cp942 | Charset::Cp942C | Charset::Cp943C)
}

pub(super) struct Encoder {
    charset: Charset,
    shifted: bool,
    pending: Option<(char, usize)>,
}

impl Encoder {
    pub(super) fn new(charset: Charset, initial_shift: bool) -> Self {
        Self {
            charset,
            shifted: initial_shift,
            pending: None,
        }
    }

    pub(super) fn encode(
        &mut self,
        input: &str,
        last: bool,
        options: EncodeOptions,
        offset: usize,
    ) -> Result<Vec<u8>, Error> {
        let mut output = Vec::new();
        let mut chars = input.char_indices().peekable();
        if let Some((character, position)) = self.pending.take() {
            if let Some(&(_, '\u{3099}')) = chars.peek() {
                let code = table::ENCODE_JEF_SEQUENCES
                    .get(character as u32, 0x3099)
                    .unwrap()
                    .0;
                chars.next();
                output.extend_from_slice(&(code as u16).to_be_bytes());
            } else if input.is_empty() && !last {
                self.pending = Some((character, position));
            } else {
                self.scalar(character, position, options.errors, &mut output)?;
            }
        }
        while let Some((index, character)) = chars.next() {
            if self.charset == Charset::Jef {
                if let Some((code, _)) = table::ENCODE_JEF_SEQUENCES.get(character as u32, 0x3099) {
                    if let Some(&(_, '\u{3099}')) = chars.peek() {
                        chars.next();
                        output.extend_from_slice(&(code as u16).to_be_bytes());
                        continue;
                    } else if chars.peek().is_none() && !last {
                        self.pending = Some((character, offset + index));
                        break;
                    }
                }
            }
            self.scalar(character, offset + index, options.errors, &mut output)?;
        }
        if last && is_mixed(self.charset) && self.shifted && !options.initial_shift {
            output.push(0x0f);
            self.shifted = false;
        }
        Ok(output)
    }

    fn scalar(
        &mut self,
        character: char,
        offset: usize,
        policy: ErrorPolicy,
        output: &mut Vec<u8>,
    ) -> Result<(), Error> {
        let unicode = character as u32;
        let code = if self.charset == Charset::Jef && (0xe000..=0xec1d).contains(&unicode) {
            let delta = unicode - 0xe000;
            Some((((0x80 + delta / 94) << 8) | (0xa1 + delta % 94), 0))
        } else {
            tables(self.charset).0.get(unicode, 0)
        };
        if let Some((code, _)) = code {
            if self.charset == Charset::Jef {
                output.extend_from_slice(&(code as u16).to_be_bytes());
            } else if is_mixed(self.charset) {
                self.set_shift(false, output);
                output.push(code as u8);
            } else if code > 0xff {
                output.extend_from_slice(&(code as u16).to_be_bytes());
            } else {
                output.push(code as u8);
            }
        } else if is_mixed(self.charset) {
            if let Some((code, _)) = table::ENCODE_CP11684.get(unicode, 0) {
                self.set_shift(true, output);
                output.extend_from_slice(&(code as u16).to_be_bytes());
            } else {
                self.unmappable(character, offset, policy, output)?;
            }
        } else {
            self.unmappable(character, offset, policy, output)?;
        }
        Ok(())
    }

    fn set_shift(&mut self, value: bool, output: &mut Vec<u8>) {
        if self.shifted != value {
            output.push(if value { 0x0e } else { 0x0f });
            self.shifted = value;
        }
    }

    fn unmappable(
        &mut self,
        character: char,
        offset: usize,
        policy: ErrorPolicy,
        output: &mut Vec<u8>,
    ) -> Result<(), Error> {
        if policy == ErrorPolicy::Strict {
            return Err(Error::Unmappable {
                charset: self.charset.name(),
                character,
                offset,
            });
        }
        if self.charset == Charset::Jef {
            output.extend_from_slice(&[0x40, 0x40]);
        } else if is_mixed(self.charset) {
            self.set_shift(false, output);
            output.push(0x6f);
        } else if self.charset.is_ebcdic() {
            output.push(0x6f);
        } else {
            output.push(b'?');
        }
        Ok(())
    }
}

pub(super) struct Decoder {
    charset: Charset,
    shifted: bool,
    pending: Option<(u8, usize)>,
}

impl Decoder {
    pub(super) fn new(charset: Charset, initial_shift: bool) -> Self {
        Self {
            charset,
            shifted: initial_shift,
            pending: None,
        }
    }

    pub(super) fn decode(
        &mut self,
        input: &[u8],
        last: bool,
        policy: ErrorPolicy,
        offset: usize,
    ) -> Result<String, Error> {
        let mut output = String::new();
        let mut index = 0;
        while index < input.len() {
            let byte = input[index];
            if let Some((lead, position)) = self.pending.take() {
                if (is_mixed(self.charset) && matches!(byte, 0x0e | 0x0f))
                    || (is_sjis(self.charset) && !(0x40..=0xfc).contains(&byte))
                    || (is_sjis(self.charset) && byte == 0x7f)
                {
                    self.invalid(position, policy, &mut output)?;
                    // Recover ASCII/control bytes as the start of a new unit.
                    if byte <= 0x7f {
                        continue;
                    }
                    index += 1;
                    continue;
                }
                index += 1;
                let code = (u32::from(lead) << 8) | u32::from(byte);
                let value = if self.charset == Charset::Jef
                    && (0x80..=0xa0).contains(&lead)
                    && (0xa1..=0xfe).contains(&byte)
                {
                    Some((
                        0xe000 + u32::from(lead - 0x80) * 94 + u32::from(byte - 0xa1),
                        0,
                    ))
                } else if is_mixed(self.charset) {
                    table::DECODE_CP11684.get(code, 0)
                } else {
                    tables(self.charset).1.get(code, 0)
                };
                if let Some((first, second)) = value {
                    output.push(char::from_u32(first).unwrap());
                    if second != 0 {
                        output.push(char::from_u32(second).unwrap());
                    }
                } else {
                    self.invalid(position, policy, &mut output)?;
                }
                continue;
            }
            if is_mixed(self.charset) && matches!(byte, 0x0e | 0x0f) {
                self.shifted = byte == 0x0e;
            } else if self.charset == Charset::Jef
                || (is_mixed(self.charset) && self.shifted)
                || (is_sjis(self.charset) && (matches!(byte, 0x81..=0x9f | 0xe0..=0xfc)))
            {
                self.pending = Some((byte, offset + index));
            } else if let Some((unicode, _)) = tables(self.charset).1.get(u32::from(byte), 0) {
                output.push(char::from_u32(unicode).unwrap());
            } else {
                self.invalid(offset + index, policy, &mut output)?;
            }
            index += 1;
        }
        if last {
            if let Some((_, position)) = self.pending.take() {
                self.invalid(position, policy, &mut output)?;
            }
        }
        Ok(output)
    }

    fn invalid(
        &self,
        offset: usize,
        policy: ErrorPolicy,
        output: &mut String,
    ) -> Result<(), Error> {
        if policy == ErrorPolicy::Strict {
            Err(Error::MalformedInput {
                charset: self.charset.name(),
                offset,
            })
        } else {
            output.push('\u{fffd}');
            Ok(())
        }
    }
}
