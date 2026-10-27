//! Byte layouts, mainframe storage, padding and record framing.

use jtc_utils::{
    charset::*,
    decimal::*,
    fixed::{self, Alignment, Field, Layout, TextField, Trim, Value},
};
use std::io::{self, Read, Write};

fn layout() -> Layout {
    let mut kana = TextField::new(10, SHIFT_JIS);
    kana.trim = Trim::End;
    let mut kanji = TextField::new(8, JEF);
    kanji.trim = Trim::End;
    Layout::new(vec![
        Field::Text(kana),
        Field::Text(kanji),
        Field::Packed {
            width: 5,
            sign: PackedSign::Signed,
        },
        Field::Zoned {
            width: 7,
            zone: Zone::Ebcdic,
            sign: ZonedSign::Leading,
        },
        Field::Bytes(2),
    ])
    .unwrap()
}

fn record() -> Vec<Value> {
    vec![
        "ﾔﾏﾀﾞ ﾀﾛｳ".into(),
        "山田".into(),
        Value::Decimal(-123456789),
        Value::Decimal(-1234),
        vec![0x12, 0x34].into(),
    ]
}

#[test]
fn mixed_encodings_and_exact_decimal_round_trip() {
    let schema = layout();
    let values = record();
    let bytes = schema.encode(&values).unwrap();
    assert_eq!(schema.record_len(), 32);
    assert_eq!(bytes.len(), 32);
    assert_eq!(
        &bytes[10..18],
        &[JEF.encode("山田").unwrap(), vec![0x40, 0x40, 0x40, 0x40]].concat()
    );
    assert_eq!(&bytes[18..23], &[0x12, 0x34, 0x56, 0x78, 0x9d]);
    assert_eq!(schema.decode(&bytes).unwrap(), values);
    assert!(schema.decode(&bytes[..31]).is_err());
    assert!(schema.encode(&values[..4]).is_err());
    let mut bad = values.clone();
    bad[0] = Value::Decimal(0);
    assert!(matches!(
        schema.encode(&bad),
        Err(fixed::Error::Field {
            index: 0,
            source: fixed::FieldError::TypeMismatch
        })
    ));
}

#[test]
fn padding_preserves_multibyte_and_stateful_encodings() {
    for (charset, text, padding, width) in [
        (SHIFT_JIS, "あ", ' ', 5),
        (JEF, "あ", '　', 6),
        (UTF_16LE, "😀", ' ', 10),
        (Charset::Cp939, "漢", ' ', 8),
        (Charset::Standard(encoding_rs::ISO_2022_JP), "漢", ' ', 12),
    ] {
        for alignment in [Alignment::Left, Alignment::Right] {
            let mut field = TextField::new(width, charset);
            field.padding = padding;
            field.alignment = alignment;
            field.trim = Trim::Both;
            let schema = Layout::new(vec![Field::Text(field)]).unwrap();
            let bytes = schema.encode(&[text.into()]).unwrap();
            assert_eq!(bytes.len(), width);
            assert_eq!(schema.decode(&bytes).unwrap(), [Value::from(text)]);
        }
    }
    // Double-byte IBM fields can transition to ASCII padding without splitting a character.
    let mut field = TextField::new(8, Charset::Cp939);
    field.initial_shift = true;
    field.trim = Trim::Both;
    let schema = Layout::new(vec![Field::Text(field)]).unwrap();
    let bytes = schema.encode(&["漢".into()]).unwrap();
    assert_eq!(schema.decode(&bytes).unwrap(), [Value::from("漢")]);
    let schema = Layout::new(vec![Field::Text(TextField::new(3, JEF))]).unwrap();
    assert!(matches!(
        schema.encode(&["あ".into()]),
        Err(fixed::Error::Field {
            source: fixed::FieldError::Padding,
            ..
        })
    ));
    let schema = Layout::new(vec![Field::Text(TextField::new(3, SHIFT_JIS))]).unwrap();
    assert!(matches!(
        schema.encode(&["あい".into()]),
        Err(fixed::Error::Field {
            source: fixed::FieldError::Overflow { .. },
            ..
        })
    ));
    assert!(schema.encode(&["😀".into()]).is_err());
}

struct OneByte<R>(R);
impl<R: Read> Read for OneByte<R> {
    fn read(&mut self, output: &mut [u8]) -> io::Result<usize> {
        let size = 1.min(output.len());
        self.0.read(&mut output[..size])
    }
}

#[test]
fn records_separators_partial_eof_and_recoverable_conversion_errors() {
    let schema = layout();
    let values = record();
    let mut writer =
        fixed::Writer::new(Vec::new(), schema.clone()).with_separator(b"\r\n".to_vec());
    assert!(writer.write_record(&[]).is_err());
    assert!(writer.get_ref().is_empty());
    writer.write_record(&values).unwrap();
    writer.write_record(&values).unwrap();
    assert_eq!(writer.record_count(), 2);
    let bytes = writer.finish().unwrap();
    let mut reader = fixed::Reader::new(OneByte(bytes.as_slice()), schema.clone())
        .with_separator(b"\r\n".to_vec());
    assert_eq!(reader.read_record().unwrap(), Some(values.clone()));
    assert_eq!(reader.read_record().unwrap(), Some(values.clone()));
    assert_eq!(reader.record_count(), 2);
    assert!(reader.next().is_none());
    assert!(reader.next().is_none());
    let mut partial = fixed::Reader::new(&bytes[..bytes.len() - 3], schema.clone())
        .with_separator(b"\r\n".to_vec());
    assert!(partial.next().unwrap().is_ok());
    assert!(matches!(
        partial.next().unwrap(),
        Err(fixed::Error::RecordLength {
            expected: 32,
            actual: 31
        })
    ));
    assert!(partial.next().is_none());
    let mut truncated_separator =
        fixed::Reader::new(&bytes[..33], schema.clone()).with_separator(b"\r\n".to_vec());
    assert!(matches!(
        truncated_separator.next().unwrap(),
        Err(fixed::Error::InvalidSeparator)
    ));
    let raw = schema.encode(&values).unwrap();
    let rows: Vec<_> = fixed::Reader::new([raw.clone(), raw].concat().as_slice(), schema)
        .collect::<Result<_, _>>()
        .unwrap();
    assert_eq!(rows, [values.clone(), values]);
}

#[test]
fn invalid_layouts_and_bom_inside_field() {
    assert!(Layout::new(vec![]).is_err());
    assert!(Layout::new(vec![Field::Bytes(0)]).is_err());
    assert!(Layout::new(vec![Field::Bytes(usize::MAX), Field::Bytes(1)]).is_err());
    assert!(
        Layout::new(vec![Field::Zoned {
            width: 1,
            zone: Zone::Ascii,
            sign: ZonedSign::Leading
        }])
        .is_err()
    );
    let schema = Layout::new(vec![Field::Text(TextField::new(6, UTF_8))]).unwrap();
    assert_eq!(
        schema.decode("\u{feff}あ".as_bytes()).unwrap(),
        [Value::from("\u{feff}あ")]
    );
}

struct Broken;
impl Write for Broken {
    fn write(&mut self, _: &[u8]) -> io::Result<usize> {
        Err(io::ErrorKind::BrokenPipe.into())
    }
    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

#[test]
fn io_error_permanently_fails_writer() {
    let mut writer = fixed::Writer::new(Broken, layout());
    assert!(matches!(
        writer.write_record(&record()),
        Err(fixed::Error::Io(_))
    ));
    assert_eq!(writer.record_count(), 0);
    assert!(matches!(
        writer.write_record(&record()),
        Err(fixed::Error::Failed)
    ));
    assert!(matches!(writer.finish(), Err(fixed::Error::Failed)));
}
