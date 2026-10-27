//! Byte-oriented mainframe records with per-field encodings and exact decimal
//! storage. Generic CSV, numeric formatting and binary primitives remain the
//! responsibility of existing crates or the standard library.

use crate::{
    charset::{self, BomHandling, Charset, DecodeOptions, EncodeOptions},
    decimal::{self, PackedSign, Zone, ZonedSign},
};
use std::{
    error, fmt,
    io::{self, Read, Write},
};

/// Placement of a text field's padding.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Alignment {
    /// Text first, then padding.
    #[default]
    Left,
    /// Padding first, then text.
    Right,
}

/// Unicode whitespace trimming after a text field is decoded.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Trim {
    /// Preserve all decoded characters.
    #[default]
    None,
    /// Remove leading whitespace.
    Start,
    /// Remove trailing whitespace.
    End,
    /// Remove leading and trailing whitespace.
    Both,
}

/// A byte-width text field with its own encoding.
#[derive(Clone, Debug)]
pub struct TextField {
    /// Width in encoded bytes, not Unicode characters.
    pub width: usize,
    /// The field encoding.
    pub charset: Charset,
    /// Padding placement.
    pub alignment: Alignment,
    /// Padding scalar; it must encode in this field's charset.
    pub padding: char,
    /// Whitespace trimming on decode.
    pub trim: Trim,
    /// Begin CP930/CP939 fields in double-byte mode; omit final SI on encode.
    pub initial_shift: bool,
}

impl TextField {
    /// Left aligned with no trimming. Padding defaults to ASCII space, or
    /// ideographic space for JEF, which cannot encode ASCII space.
    pub fn new(width: usize, charset: Charset) -> Self {
        Self {
            width,
            charset,
            alignment: Alignment::Left,
            padding: if charset == charset::JEF {
                '\u{3000}'
            } else {
                ' '
            },
            trim: Trim::None,
            initial_shift: false,
        }
    }

    fn encode(&self, input: &str) -> Result<Vec<u8>, FieldError> {
        let encode = |text: &str| {
            self.charset
                .encoder_with_options(EncodeOptions {
                    initial_shift: self.initial_shift,
                    ..Default::default()
                })
                .encode(text, true)
                .map_err(FieldError::Charset)
        };
        let bytes = encode(input)?;
        if bytes.len() > self.width {
            return Err(FieldError::Overflow {
                width: self.width,
                actual: bytes.len(),
            });
        }
        if bytes.len() == self.width {
            return Ok(bytes);
        }
        let padded = |count: usize| {
            let padding: String = std::iter::repeat_n(self.padding, count).collect();
            match self.alignment {
                Alignment::Left => format!("{input}{padding}"),
                Alignment::Right => format!("{padding}{input}"),
            }
        };
        // Encode whole fields so padding never interrupts SO/SI or ISO-2022-JP
        // state. The first pad may add a transition; subsequent pads have a
        // constant byte cost. Verify the final size before returning any bytes.
        let one = encode(&padded(1))?.len();
        let two = encode(&padded(2))?.len();
        let step = two
            .checked_sub(one)
            .filter(|&n| n > 0)
            .ok_or(FieldError::Padding)?;
        if one > self.width || (self.width - one) % step != 0 {
            return Err(FieldError::Padding);
        }
        let bytes = encode(&padded(1 + (self.width - one) / step))?;
        if bytes.len() != self.width {
            return Err(FieldError::Padding);
        }
        Ok(bytes)
    }
}

/// A field's on-disk representation. Fields are laid out consecutively; use
/// [`Field::Bytes`] to preserve reserved areas or access standard binary types.
#[derive(Clone, Debug)]
pub enum Field {
    /// Encoded text.
    Text(TextField),
    /// Packed BCD with an exact integral coefficient.
    Packed {
        /// Encoded byte width.
        width: usize,
        /// Packed sign representation.
        sign: PackedSign,
    },
    /// Zoned decimal with an exact integral coefficient.
    Zoned {
        /// Encoded byte width, including separate sign if configured.
        width: usize,
        /// ASCII or EBCDIC zone/sign conventions.
        zone: Zone,
        /// Sign position.
        sign: ZonedSign,
    },
    /// Opaque bytes, with the given width.
    Bytes(usize),
}

impl Field {
    /// Encoded byte width.
    pub fn width(&self) -> usize {
        match self {
            Self::Text(field) => field.width,
            Self::Packed { width, .. } | Self::Zoned { width, .. } | Self::Bytes(width) => *width,
        }
    }
}

/// A typed field value. Decimal values are unscaled coefficients, so no precision
/// is lost through floating point. Applications can apply a schema's implied scale.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Value {
    /// Unicode text.
    Text(String),
    /// Integral packed/zoned decimal coefficient.
    Decimal(i128),
    /// Opaque bytes.
    Bytes(Vec<u8>),
}

impl From<&str> for Value {
    fn from(value: &str) -> Self {
        Self::Text(value.into())
    }
}
impl From<String> for Value {
    fn from(value: String) -> Self {
        Self::Text(value)
    }
}
impl From<i128> for Value {
    fn from(value: i128) -> Self {
        Self::Decimal(value)
    }
}
impl From<Vec<u8>> for Value {
    fn from(value: Vec<u8>) -> Self {
        Self::Bytes(value)
    }
}

/// A conversion error within a particular field.
#[derive(Debug)]
pub enum FieldError {
    /// The supplied value type does not match the schema.
    TypeMismatch,
    /// Text or opaque bytes exceed the field width.
    Overflow {
        /// Schema byte width.
        width: usize,
        /// Supplied encoded byte count.
        actual: usize,
    },
    /// The padding scalar cannot fill the remaining byte count exactly.
    Padding,
    /// Invalid or unrepresentable text.
    Charset(charset::Error),
    /// Invalid decimal storage or capacity.
    Decimal(decimal::Error),
}

impl fmt::Display for FieldError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::TypeMismatch => f.write_str("value type does not match field"),
            Self::Overflow { width, actual } => {
                write!(f, "field requires {width} bytes, got {actual}")
            }
            Self::Padding => f.write_str("padding cannot fill the field exactly"),
            Self::Charset(error) => error.fmt(f),
            Self::Decimal(error) => error.fmt(f),
        }
    }
}
impl error::Error for FieldError {
    fn source(&self) -> Option<&(dyn error::Error + 'static)> {
        match self {
            Self::Charset(error) => Some(error),
            Self::Decimal(error) => Some(error),
            _ => None,
        }
    }
}

/// Invalid layout, record, field, or I/O failure.
#[derive(Debug)]
pub enum Error {
    /// A layout must contain nonzero-width fields without size overflow.
    InvalidLayout(&'static str),
    /// A record has the wrong byte length or is truncated at EOF.
    RecordLength {
        /// Schema byte length.
        expected: usize,
        /// Actual number of bytes.
        actual: usize,
    },
    /// Value count differs from the schema.
    FieldCount {
        /// Number of fields in the schema.
        expected: usize,
        /// Number of supplied values.
        actual: usize,
    },
    /// Error at a zero-based field index.
    Field {
        /// Zero-based field index.
        index: usize,
        /// Underlying conversion error.
        source: FieldError,
    },
    /// Record separator differs from the configured bytes or is truncated.
    InvalidSeparator,
    /// A stream was already used after an I/O failure.
    Failed,
    /// Underlying I/O failure.
    Io(io::Error),
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidLayout(message) => write!(f, "invalid layout: {message}"),
            Self::RecordLength { expected, actual } => {
                write!(f, "record needs {expected} bytes, got {actual}")
            }
            Self::FieldCount { expected, actual } => {
                write!(f, "record needs {expected} values, got {actual}")
            }
            Self::Field { index, source } => write!(f, "field {index}: {source}"),
            Self::InvalidSeparator => f.write_str("invalid or truncated record separator"),
            Self::Failed => f.write_str("record stream failed previously"),
            Self::Io(error) => error.fmt(f),
        }
    }
}
impl error::Error for Error {
    fn source(&self) -> Option<&(dyn error::Error + 'static)> {
        match self {
            Self::Field { source, .. } => Some(source),
            Self::Io(error) => Some(error),
            _ => None,
        }
    }
}
impl From<io::Error> for Error {
    fn from(error: io::Error) -> Self {
        Self::Io(error)
    }
}

/// Validated, reusable layout. Use another layout for another record type;
/// applications may inspect a discriminator via byte slices before decoding.
#[derive(Clone, Debug)]
pub struct Layout {
    fields: Vec<Field>,
    record_len: usize,
}

impl Layout {
    /// Validate the field widths and text padding encodability.
    pub fn new(fields: Vec<Field>) -> Result<Self, Error> {
        if fields.is_empty() {
            return Err(Error::InvalidLayout("no fields"));
        }
        let mut record_len = 0usize;
        for (index, field) in fields.iter().enumerate() {
            if field.width() == 0 {
                return Err(Error::InvalidLayout("zero-width field"));
            }
            record_len = record_len
                .checked_add(field.width())
                .ok_or(Error::InvalidLayout("record size overflow"))?;
            if record_len > isize::MAX as usize {
                return Err(Error::InvalidLayout(
                    "record exceeds addressable byte length",
                ));
            }
            match field {
                Field::Text(text) => {
                    text.charset
                        .encode(&text.padding.to_string())
                        .map_err(|error| Error::Field {
                            index,
                            source: FieldError::Charset(error),
                        })?;
                }
                Field::Zoned {
                    width: 1,
                    sign: ZonedSign::Leading | ZonedSign::Trailing,
                    ..
                } => {
                    return Err(Error::InvalidLayout(
                        "separate sign needs space for a digit",
                    ));
                }
                _ => {}
            }
        }
        Ok(Self { fields, record_len })
    }

    /// Record byte length, excluding any stream-level separator.
    pub fn record_len(&self) -> usize {
        self.record_len
    }

    /// Inspect the validated fields.
    pub fn fields(&self) -> &[Field] {
        &self.fields
    }

    /// Encode a record. Oversized text is rejected; multibyte characters are
    /// never cut. The whole record is validated before a writer emits it.
    pub fn encode(&self, values: &[Value]) -> Result<Vec<u8>, Error> {
        if values.len() != self.fields.len() {
            return Err(Error::FieldCount {
                expected: self.fields.len(),
                actual: values.len(),
            });
        }
        let mut output = Vec::with_capacity(self.record_len);
        for (index, (field, value)) in self.fields.iter().zip(values).enumerate() {
            let result = match (field, value) {
                (Field::Text(text), Value::Text(value)) => text.encode(value),
                (Field::Packed { width, sign }, Value::Decimal(value)) => {
                    decimal::encode_packed(*value, *width, *sign).map_err(FieldError::Decimal)
                }
                (Field::Zoned { width, zone, sign }, Value::Decimal(value)) => {
                    decimal::encode_zoned(*value, *width, *zone, *sign).map_err(FieldError::Decimal)
                }
                (Field::Bytes(width), Value::Bytes(value)) => {
                    if value.len() == *width {
                        Ok(value.clone())
                    } else {
                        Err(FieldError::Overflow {
                            width: *width,
                            actual: value.len(),
                        })
                    }
                }
                _ => Err(FieldError::TypeMismatch),
            };
            output.extend(result.map_err(|source| Error::Field { index, source })?);
        }
        Ok(output)
    }

    /// Decode exactly one record; each text field starts a new conversion state.
    pub fn decode(&self, input: &[u8]) -> Result<Vec<Value>, Error> {
        if input.len() != self.record_len {
            return Err(Error::RecordLength {
                expected: self.record_len,
                actual: input.len(),
            });
        }
        let mut output = Vec::with_capacity(self.fields.len());
        let mut start = 0;
        for (index, field) in self.fields.iter().enumerate() {
            let bytes = &input[start..start + field.width()];
            let value = match field {
                Field::Text(text) => text
                    .charset
                    .decoder_with_options(DecodeOptions {
                        bom: BomHandling::Preserve,
                        initial_shift: text.initial_shift,
                        ..Default::default()
                    })
                    .decode(bytes, true)
                    .map(|decoded| {
                        Value::Text(match text.trim {
                            Trim::None => decoded,
                            Trim::Start => decoded.trim_start().into(),
                            Trim::End => decoded.trim_end().into(),
                            Trim::Both => decoded.trim().into(),
                        })
                    })
                    .map_err(FieldError::Charset),
                Field::Packed { sign, .. } => decimal::decode_packed(bytes, *sign)
                    .map(Value::Decimal)
                    .map_err(FieldError::Decimal),
                Field::Zoned { zone, sign, .. } => decimal::decode_zoned(bytes, *zone, *sign)
                    .map(Value::Decimal)
                    .map_err(FieldError::Decimal),
                Field::Bytes(_) => Ok(Value::Bytes(bytes.to_vec())),
            };
            output.push(value.map_err(|source| Error::Field { index, source })?);
            start += field.width();
        }
        Ok(output)
    }
}

fn read_bytes(reader: &mut impl Read, output: &mut [u8]) -> io::Result<usize> {
    let mut size = 0;
    while size < output.len() {
        match reader.read(&mut output[size..]) {
            Ok(0) => break,
            Ok(read) => size += read,
            Err(error) if error.kind() == io::ErrorKind::Interrupted => {}
            Err(error) => return Err(error),
        }
    }
    Ok(size)
}

/// Read consecutive byte-width records, rejecting a partial final record.
pub struct Reader<R> {
    inner: R,
    layout: Layout,
    separator: Vec<u8>,
    count: u64,
    done: bool,
}

impl<R: Read> Reader<R> {
    /// Create a reader with no separator or implicit BOM handling.
    pub fn new(inner: R, layout: Layout) -> Self {
        Self {
            inner,
            layout,
            separator: Vec::new(),
            count: 0,
            done: false,
        }
    }

    /// Set the exact bytes required after every record, including the last one.
    pub fn with_separator(mut self, separator: impl Into<Vec<u8>>) -> Self {
        self.separator = separator.into();
        self
    }

    /// Read a record; only EOF at a record boundary returns `None`.
    /// After EOF or any error the reader is exhausted.
    pub fn read_record(&mut self) -> Result<Option<Vec<Value>>, Error> {
        if self.done {
            return Ok(None);
        }
        let result = self.read_next();
        if !matches!(result, Ok(Some(_))) {
            self.done = true;
        }
        result
    }

    fn read_next(&mut self) -> Result<Option<Vec<Value>>, Error> {
        let mut bytes = vec![0; self.layout.record_len()];
        let size = read_bytes(&mut self.inner, &mut bytes)?;
        if size == 0 {
            return Ok(None);
        }
        if size != bytes.len() {
            return Err(Error::RecordLength {
                expected: bytes.len(),
                actual: size,
            });
        }
        if !self.separator.is_empty() {
            let mut separator = vec![0; self.separator.len()];
            if read_bytes(&mut self.inner, &mut separator)? != separator.len()
                || separator != self.separator
            {
                return Err(Error::InvalidSeparator);
            }
        }
        let values = self.layout.decode(&bytes)?;
        self.count += 1;
        Ok(Some(values))
    }

    /// Number of successfully decoded records.
    pub fn record_count(&self) -> u64 {
        self.count
    }

    /// Recover the source at its current byte position.
    pub fn into_inner(self) -> R {
        self.inner
    }
}

impl<R: Read> Iterator for Reader<R> {
    type Item = Result<Vec<Value>, Error>;
    fn next(&mut self) -> Option<Self::Item> {
        self.read_record().transpose()
    }
}
impl<R: Read> std::iter::FusedIterator for Reader<R> {}

/// Write validated records to any `Write` destination.
pub struct Writer<W> {
    inner: W,
    layout: Layout,
    separator: Vec<u8>,
    count: u64,
    failed: bool,
}

impl<W: Write> Writer<W> {
    /// Create a writer without separators or an implicit BOM.
    pub fn new(inner: W, layout: Layout) -> Self {
        Self {
            inner,
            layout,
            separator: Vec::new(),
            count: 0,
            failed: false,
        }
    }

    /// Append these exact bytes after every record, including the final one.
    pub fn with_separator(mut self, separator: impl Into<Vec<u8>>) -> Self {
        self.separator = separator.into();
        self
    }

    /// Validate and write one complete record. Conversion errors emit no bytes.
    /// I/O errors may leave partial output and permanently fail this writer.
    pub fn write_record(&mut self, values: &[Value]) -> Result<(), Error> {
        if self.failed {
            return Err(Error::Failed);
        }
        let bytes = self.layout.encode(values)?;
        let result = self
            .inner
            .write_all(&bytes)
            .and_then(|()| self.inner.write_all(&self.separator));
        if let Err(error) = result {
            self.failed = true;
            return Err(Error::Io(error));
        }
        self.count += 1;
        Ok(())
    }

    /// Number of completely written records.
    pub fn record_count(&self) -> u64 {
        self.count
    }

    /// Flush buffered output and recover the destination.
    pub fn finish(mut self) -> Result<W, Error> {
        if self.failed {
            return Err(Error::Failed);
        }
        self.inner.flush()?;
        Ok(self.inner)
    }

    /// Borrow the destination, for inspecting a memory buffer for example.
    pub fn get_ref(&self) -> &W {
        &self.inner
    }
}
