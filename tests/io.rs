//! Standard I/O integration, bytewise UTF-8 and downstream failures.

use jtc_utils::{
    charset::*,
    io::{DecodeReader, EncodeWriter},
};
use std::io::{self, Read, Write};

struct Chunked<R> {
    inner: R,
    size: usize,
    interrupted: bool,
}
impl<R: Read> Read for Chunked<R> {
    fn read(&mut self, output: &mut [u8]) -> io::Result<usize> {
        if !self.interrupted {
            self.interrupted = true;
            return Err(io::ErrorKind::Interrupted.into());
        }
        self.interrupted = false;
        let size = self.size.min(output.len());
        self.inner.read(&mut output[..size])
    }
}

#[test]
fn readers_and_writers_handle_single_byte_chunks() {
    for (charset, text) in [
        (SHIFT_JIS, "AあいうｱｲｳB"),
        (EUC_JP, "AあいうB"),
        (JEF, "あ海\u{1b019}\u{3099}\u{e000}"),
        (Charset::Cp930, "Aあ海B"),
        (Charset::Cp939, "Aあ海B"),
        (UTF_16LE, "A😀あ"),
        (UTF_16BE, "A😀あ"),
        (Charset::Standard(encoding_rs::ISO_2022_JP), "Aあ海B"),
    ] {
        let expected = charset.encode(text).unwrap();
        let mut writer = EncodeWriter::new(Vec::new(), charset);
        for byte in text.as_bytes() {
            writer.write_all(&[*byte]).unwrap();
            writer.flush().unwrap();
        }
        let actual = writer.finish().unwrap();
        assert_eq!(actual, expected, "{}", charset.name());
        let input = Chunked {
            inner: actual.as_slice(),
            size: 1,
            interrupted: false,
        };
        let mut reader = DecodeReader::new(input, charset);
        let mut decoded = String::new();
        reader.read_to_string(&mut decoded).unwrap();
        assert_eq!(decoded, charset.decode(&expected).unwrap());
        assert_eq!(reader.read(&mut [0; 1]).unwrap(), 0);
        assert_eq!(reader.read(&mut []).unwrap(), 0);
    }
}

#[test]
fn large_chunks_cross_internal_output_buffers() {
    let text = "あA漢B".repeat(20_000);
    for charset in [
        SHIFT_JIS,
        Charset::Cp939,
        Charset::Standard(encoding_rs::ISO_2022_JP),
    ] {
        let mut writer = EncodeWriter::new(Vec::new(), charset);
        writer.write_all(text.as_bytes()).unwrap();
        let bytes = writer.finish().unwrap();
        assert_eq!(bytes, charset.encode(&text).unwrap());
        let mut reader = DecodeReader::new(bytes.as_slice(), charset);
        let mut decoded = String::new();
        reader.read_to_string(&mut decoded).unwrap();
        assert_eq!(decoded, text);
    }
}

#[test]
fn csv_crate_reads_and_writes_legacy_text() {
    for charset in [SHIFT_JIS, Charset::Cp939, Charset::Cp930] {
        let transcoder = EncodeWriter::new(Vec::new(), charset);
        let mut writer = csv::WriterBuilder::new()
            .terminator(csv::Terminator::CRLF)
            .from_writer(transcoder);
        writer.write_record(["名前", "説明"]).unwrap();
        writer
            .write_record(["山田　太郎", "引用\"と,区切り\n改行"])
            .unwrap();
        let bytes = writer
            .into_inner()
            .unwrap_or_else(|error| panic!("{error}"))
            .finish()
            .unwrap();
        assert!(
            charset
                .decode(&bytes)
                .unwrap()
                .contains("\"引用\"\"と,区切り\n改行\"")
        );
        let transcoder = DecodeReader::new(bytes.as_slice(), charset);
        let mut reader = csv::Reader::from_reader(transcoder);
        assert_eq!(
            reader.headers().unwrap(),
            &csv::StringRecord::from(vec!["名前", "説明"])
        );
        let records: Vec<_> = reader.records().collect::<Result<_, _>>().unwrap();
        assert_eq!(
            records,
            [csv::StringRecord::from(vec![
                "山田　太郎",
                "引用\"と,区切り\n改行"
            ])]
        );
    }
}

#[test]
fn malformed_utf8_and_trailing_input_are_reported() {
    let mut writer = EncodeWriter::new(Vec::new(), SHIFT_JIS);
    writer.write_all(&[0xe3, 0x81]).unwrap();
    assert_eq!(
        writer.finish().unwrap_err().kind(),
        io::ErrorKind::InvalidData
    );
    let mut writer = EncodeWriter::new(Vec::new(), SHIFT_JIS);
    assert_eq!(
        writer.write(&[0xff]).unwrap_err().kind(),
        io::ErrorKind::InvalidData
    );
    assert!(writer.write(b"valid").is_err());
    assert!(writer.finish().is_err());
    let mut writer = EncodeWriter::new(Vec::new(), SHIFT_JIS);
    assert_eq!(
        writer.write("😀".as_bytes()).unwrap_err().kind(),
        io::ErrorKind::InvalidData
    );
    let mut reader = DecodeReader::new(&[0xa4][..], JEF);
    assert_eq!(
        reader
            .read_to_string(&mut String::new())
            .unwrap_err()
            .kind(),
        io::ErrorKind::InvalidData
    );
    assert_eq!(reader.read(&mut [0; 1]).unwrap(), 0);
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
fn failed_output_cannot_be_reused() {
    let mut writer = EncodeWriter::new(Broken, SHIFT_JIS);
    assert_eq!(
        writer.write(b"a").unwrap_err().kind(),
        io::ErrorKind::BrokenPipe
    );
    assert!(writer.write(b"b").is_err());
    assert!(writer.flush().is_err());
    assert!(writer.finish().is_err());
}
