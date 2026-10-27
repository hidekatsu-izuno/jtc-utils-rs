use super::{Error, ErrorPolicy};
use encoding_rs::{DecoderResult, EncoderResult, Encoding};

pub(super) enum Encoder {
    Standard(encoding_rs::Encoder),
    Utf16 { little_endian: bool },
    Unsupported,
}

impl Encoder {
    pub(super) fn new(encoding: &'static Encoding) -> Self {
        if encoding == encoding_rs::UTF_16LE || encoding == encoding_rs::UTF_16BE {
            Self::Utf16 {
                little_endian: encoding == encoding_rs::UTF_16LE,
            }
        } else if encoding == encoding_rs::REPLACEMENT {
            Self::Unsupported
        } else {
            Self::Standard(encoding.new_encoder())
        }
    }

    pub(super) fn encode(
        &mut self,
        input: &str,
        last: bool,
        policy: ErrorPolicy,
        offset: usize,
    ) -> Result<Vec<u8>, Error> {
        let Self::Standard(encoder) = self else {
            return match self {
                Self::Utf16 { little_endian } => Ok(input
                    .encode_utf16()
                    .flat_map(|unit| {
                        if *little_endian {
                            unit.to_le_bytes()
                        } else {
                            unit.to_be_bytes()
                        }
                    })
                    .collect()),
                Self::Unsupported => Err(Error::UnsupportedEncoding("replacement")),
                _ => unreachable!(),
            };
        };
        let mut output = Vec::new();
        let mut read_total = 0;
        loop {
            let mut buffer = [0; 4096];
            let (result, read, written) = encoder.encode_from_utf8_without_replacement(
                &input[read_total..],
                &mut buffer,
                last,
            );
            read_total += read;
            output.extend_from_slice(&buffer[..written]);
            match result {
                EncoderResult::InputEmpty => return Ok(output),
                EncoderResult::OutputFull => {}
                EncoderResult::Unmappable(character) => {
                    if policy == ErrorPolicy::Strict {
                        return Err(Error::Unmappable {
                            charset: encoder.encoding().name(),
                            character,
                            offset: offset + read_total - character.len_utf8(),
                        });
                    }
                    // Use the encoder itself for '?' so ISO-2022-JP emits the
                    // necessary escape back to ASCII. Never emit HTML entities.
                    let (result, read, written) =
                        encoder.encode_from_utf8_without_replacement("?", &mut buffer, false);
                    debug_assert_eq!(result, EncoderResult::InputEmpty);
                    debug_assert_eq!(read, 1);
                    output.extend_from_slice(&buffer[..written]);
                }
            }
        }
    }
}

pub(super) fn decode(
    decoder: &mut encoding_rs::Decoder,
    input: &[u8],
    last: bool,
    policy: ErrorPolicy,
    offset: usize,
) -> Result<String, Error> {
    let mut output = String::new();
    let mut read_total = 0;
    loop {
        let mut buffer = [0; 4096];
        let (result, read, written) =
            decoder.decode_to_utf8_without_replacement(&input[read_total..], &mut buffer, last);
        read_total += read;
        // encoding_rs guarantees that every returned output chunk is UTF-8.
        output.push_str(std::str::from_utf8(&buffer[..written]).unwrap());
        match result {
            DecoderResult::InputEmpty => return Ok(output),
            DecoderResult::OutputFull => {}
            DecoderResult::Malformed(length, after) => {
                if policy == ErrorPolicy::Strict {
                    return Err(Error::MalformedInput {
                        charset: decoder.encoding().name(),
                        offset: (offset + read_total)
                            .saturating_sub(usize::from(length) + usize::from(after)),
                    });
                }
                output.push('\u{fffd}');
            }
        }
    }
}
