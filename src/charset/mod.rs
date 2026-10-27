//! A common, strict-by-default interface to `encoding_rs` and legacy Japanese
//! encodings. Each encoder/decoder belongs to one stream; create a new one for
//! an independent record or field.

mod legacy;
mod standard;

use std::{error, fmt};

/// A character encoding. Standard encodings are delegated to `encoding_rs`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Charset {
    /// Any encoding supplied by `encoding_rs`.
    Standard(&'static encoding_rs::Encoding),
    /// Fujitsu JEF (two-byte kanji only, without EBCDIC shift markers).
    Jef,
    /// Fujitsu EBCDIC ASCII.
    FujitsuEbcdicAscii,
    /// Fujitsu EBCDIC kana (lowercase ASCII encodes as uppercase).
    FujitsuEbcdicKana,
    /// Fujitsu EBCDIC lowercase.
    FujitsuEbcdicLower,
    /// IBM EBCDIC kana + IBM kanji with SO/SI.
    Cp930,
    /// IBM EBCDIC Latin + IBM kanji with SO/SI.
    Cp939,
    /// IBM Shift_JIS variant CP942.
    Cp942,
    /// ASCII-compatible CP942.
    Cp942C,
    /// IBM Shift_JIS variant CP943C; distinct from Windows-31J.
    Cp943C,
}

/// UTF-8, implemented by `encoding_rs`.
pub const UTF_8: Charset = Charset::Standard(encoding_rs::UTF_8);
/// UTF-16LE; decoding uses `encoding_rs`, encoding uses `str::encode_utf16`.
pub const UTF_16LE: Charset = Charset::Standard(encoding_rs::UTF_16LE);
/// UTF-16BE; decoding uses `encoding_rs`, encoding uses `str::encode_utf16`.
pub const UTF_16BE: Charset = Charset::Standard(encoding_rs::UTF_16BE);
/// WHATWG Shift_JIS / Windows-31J, implemented by `encoding_rs`.
pub const SHIFT_JIS: Charset = Charset::Standard(encoding_rs::SHIFT_JIS);
/// Windows-31J, an alias of [`SHIFT_JIS`].
pub const WINDOWS_31J: Charset = SHIFT_JIS;
/// EUC-JP, implemented by `encoding_rs`.
pub const EUC_JP: Charset = Charset::Standard(encoding_rs::EUC_JP);
/// Fujitsu JEF kanji.
pub const JEF: Charset = Charset::Jef;

/// How malformed input and unrepresentable characters are handled.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ErrorPolicy {
    /// Return an error; no implicit data replacement.
    #[default]
    Strict,
    /// Decode as U+FFFD; encode as `?` (JEF uses ideographic space, 0x4040).
    Replace,
}

/// Handling of an initial BOM for a standard Unicode encoding.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum BomHandling {
    /// Remove a matching BOM, without changing the selected charset.
    #[default]
    Remove,
    /// Treat the BOM as text; useful for individual fixed-width fields.
    Preserve,
}

/// Decoder configuration. Defaults to strict input and matching BOM removal.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct DecodeOptions {
    /// Error handling.
    pub errors: ErrorPolicy,
    /// Initial BOM handling (standard encodings only).
    pub bom: BomHandling,
    /// Start CP930/CP939 in double-byte mode (fields without an initial SO).
    pub initial_shift: bool,
}

/// Encoder configuration. Encoders never add a BOM implicitly.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct EncodeOptions {
    /// Error handling.
    pub errors: ErrorPolicy,
    /// Start CP930/CP939 in double-byte mode and omit a final SI.
    pub initial_shift: bool,
}

/// A conversion failure. Offsets count bytes from the beginning of the stream;
/// encoding offsets refer to the UTF-8 input, decoding offsets to source bytes.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Error {
    /// The input is not well formed for this encoding.
    MalformedInput {
        /// The selected encoding.
        charset: &'static str,
        /// Source byte offset of the malformed sequence.
        offset: usize,
    },
    /// A Unicode scalar cannot be encoded.
    Unmappable {
        /// The selected encoding.
        charset: &'static str,
        /// The unrepresentable scalar.
        character: char,
        /// UTF-8 byte offset in the input stream.
        offset: usize,
    },
    /// `encoding_rs`'s replacement pseudo-encoding has no encoder.
    UnsupportedEncoding(&'static str),
    /// A stream was already finalized or failed. Create a new converter.
    Finished,
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::MalformedInput { charset, offset } => {
                write!(f, "invalid {charset} data at byte {offset}")
            }
            Self::Unmappable {
                charset,
                character,
                offset,
            } => write!(
                f,
                "{character:?} cannot be encoded as {charset} at UTF-8 byte {offset}"
            ),
            Self::UnsupportedEncoding(name) => write!(f, "{name} has no encoder"),
            Self::Finished => f.write_str("converter already finalized or failed"),
        }
    }
}

impl error::Error for Error {}

impl From<&'static encoding_rs::Encoding> for Charset {
    fn from(value: &'static encoding_rs::Encoding) -> Self {
        Self::Standard(value)
    }
}

impl Charset {
    /// Look up a legacy name or any label accepted by `encoding_rs`.
    pub fn for_label(label: &str) -> Option<Self> {
        let label = label.trim_matches(|c: char| c.is_ascii_whitespace());
        Some(match label.to_ascii_lowercase().as_str() {
            "jef" | "x-fujitsu-jef" => Self::Jef,
            "x-fujitsu-ebcdic-ascii" => Self::FujitsuEbcdicAscii,
            "x-fujitsu-ebcdic-kana" => Self::FujitsuEbcdicKana,
            "x-fujitsu-ebcdic-lower" => Self::FujitsuEbcdicLower,
            "cp930" | "ibm930" | "ibm-930" => Self::Cp930,
            "cp939" | "ibm939" | "ibm-939" => Self::Cp939,
            "cp942" | "ibm942" | "ibm-942" => Self::Cp942,
            "cp942c" | "ibm942c" | "ibm-942c" => Self::Cp942C,
            "cp943c" | "ibm943c" | "ibm-943c" => Self::Cp943C,
            "windows-31j" | "windows31j" | "cp932" => SHIFT_JIS,
            _ => Self::Standard(encoding_rs::Encoding::for_label(label.as_bytes())?),
        })
    }

    /// Canonical encoding name.
    pub fn name(self) -> &'static str {
        match self {
            Self::Standard(encoding) => encoding.name(),
            Self::Jef => "x-fujitsu-jef",
            Self::FujitsuEbcdicAscii => "x-fujitsu-ebcdic-ascii",
            Self::FujitsuEbcdicKana => "x-fujitsu-ebcdic-kana",
            Self::FujitsuEbcdicLower => "x-fujitsu-ebcdic-lower",
            Self::Cp930 => "cp930",
            Self::Cp939 => "cp939",
            Self::Cp942 => "cp942",
            Self::Cp942C => "cp942c",
            Self::Cp943C => "cp943c",
        }
    }

    /// Whether the encoding uses EBCDIC or JEF rather than ASCII-compatible bytes.
    pub fn is_ebcdic(self) -> bool {
        matches!(
            self,
            Self::Jef
                | Self::FujitsuEbcdicAscii
                | Self::FujitsuEbcdicKana
                | Self::FujitsuEbcdicLower
                | Self::Cp930
                | Self::Cp939
        )
    }

    /// Encode an independent string, strictly, without adding a BOM.
    pub fn encode(self, input: &str) -> Result<Vec<u8>, Error> {
        self.encoder().encode(input, true)
    }

    /// Decode an independent buffer, strictly, removing a matching initial BOM.
    pub fn decode(self, input: &[u8]) -> Result<String, Error> {
        self.decoder().decode(input, true)
    }

    /// Check representability using this encoding's actual strict encoder.
    /// Some legacy aliases encode successfully but do not round-trip identically.
    pub fn can_encode(self, input: &str) -> bool {
        self.encode(input).is_ok()
    }

    /// Create a strict streaming encoder.
    pub fn encoder(self) -> Encoder {
        self.encoder_with_options(EncodeOptions::default())
    }

    /// Create a streaming encoder with explicit options.
    pub fn encoder_with_options(self, options: EncodeOptions) -> Encoder {
        Encoder {
            backend: match self {
                Self::Standard(encoding) => {
                    EncoderBackend::Standard(standard::Encoder::new(encoding))
                }
                _ => EncoderBackend::Legacy(legacy::Encoder::new(self, options.initial_shift)),
            },
            charset: self,
            options,
            offset: 0,
            finished: false,
        }
    }

    /// Create a strict streaming decoder.
    pub fn decoder(self) -> Decoder {
        self.decoder_with_options(DecodeOptions::default())
    }

    /// Create a streaming decoder with explicit options.
    pub fn decoder_with_options(self, options: DecodeOptions) -> Decoder {
        Decoder {
            backend: match self {
                Self::Standard(encoding) => DecoderBackend::Standard(match options.bom {
                    BomHandling::Remove => encoding.new_decoder_with_bom_removal(),
                    BomHandling::Preserve => encoding.new_decoder_without_bom_handling(),
                }),
                _ => DecoderBackend::Legacy(legacy::Decoder::new(self, options.initial_shift)),
            },
            charset: self,
            options,
            offset: 0,
            finished: false,
        }
    }
}

enum EncoderBackend {
    Standard(standard::Encoder),
    Legacy(legacy::Encoder),
}

/// Incremental Unicode-to-bytes converter, including stateful IBM SO/SI.
pub struct Encoder {
    backend: EncoderBackend,
    charset: Charset,
    options: EncodeOptions,
    offset: usize,
    finished: bool,
}

impl Encoder {
    /// Encode a chunk. Set `last` on the final chunk to flush pending sequences
    /// and return CP930/CP939 to single-byte mode (unless `initial_shift` is set).
    /// After finalization or any error the converter cannot be reused.
    pub fn encode(&mut self, input: &str, last: bool) -> Result<Vec<u8>, Error> {
        if self.finished {
            return Err(Error::Finished);
        }
        if input.is_empty() && !last {
            return Ok(Vec::new());
        }
        let result = match &mut self.backend {
            EncoderBackend::Standard(encoder) => {
                encoder.encode(input, last, self.options.errors, self.offset)
            }
            EncoderBackend::Legacy(encoder) => {
                encoder.encode(input, last, self.options, self.offset)
            }
        };
        self.offset += input.len();
        self.finished = last || result.is_err();
        result
    }

    /// Flush the stream with an empty final chunk.
    pub fn finish(&mut self) -> Result<Vec<u8>, Error> {
        self.encode("", true)
    }

    /// Selected encoding.
    pub fn charset(&self) -> Charset {
        self.charset
    }
}

enum DecoderBackend {
    Standard(encoding_rs::Decoder),
    Legacy(legacy::Decoder),
}

/// Incremental bytes-to-Unicode converter. Partial multibyte sequences and
/// shift state are retained between calls, including the final call.
pub struct Decoder {
    backend: DecoderBackend,
    charset: Charset,
    options: DecodeOptions,
    offset: usize,
    finished: bool,
}

impl Decoder {
    /// Decode a chunk; `last` flushes pending input and checks for truncation.
    /// After finalization or any error the converter cannot be reused.
    pub fn decode(&mut self, input: &[u8], last: bool) -> Result<String, Error> {
        if self.finished {
            return Err(Error::Finished);
        }
        if input.is_empty() && !last {
            return Ok(String::new());
        }
        let result = match &mut self.backend {
            DecoderBackend::Standard(decoder) => {
                standard::decode(decoder, input, last, self.options.errors, self.offset)
            }
            DecoderBackend::Legacy(decoder) => {
                decoder.decode(input, last, self.options.errors, self.offset)
            }
        };
        self.offset += input.len();
        self.finished = last || result.is_err();
        result
    }

    /// Flush the stream with an empty final chunk.
    pub fn finish(&mut self) -> Result<String, Error> {
        self.decode(&[], true)
    }

    /// Selected encoding.
    pub fn charset(&self) -> Charset {
        self.charset
    }
}
