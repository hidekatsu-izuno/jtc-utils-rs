//! Zengin text, mixed mainframe records and CSV via the ordinary csv crate.

use jtc_utils::{
    charset::{Charset, JEF, SHIFT_JIS},
    decimal::PackedSign,
    fixed::{Field, Layout, TextField, Trim, Value},
    io::{DecodeReader, EncodeWriter},
    text::{normalize_japanese, zengin_kana},
};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let name = zengin_kana("やまだ　たろう")?;
    assert_eq!(name, "ﾔﾏﾀﾞ ﾀﾛｳ");
    assert_eq!(normalize_japanese("神か\u{3099}\r\n"), "神が\n");

    for charset in [JEF, SHIFT_JIS] {
        assert_eq!(charset.decode(&charset.encode("日本語")?)?, "日本語");
    }

    let mut name_field = TextField::new(20, SHIFT_JIS);
    name_field.trim = Trim::End;
    let mut kanji_field = TextField::new(8, JEF);
    kanji_field.trim = Trim::End;
    let layout = Layout::new(vec![
        Field::Text(name_field),
        Field::Text(kanji_field),
        Field::Packed {
            width: 6,
            sign: PackedSign::Signed,
        },
    ])?;
    let record = vec![name.into(), "山田太郎".into(), Value::Decimal(12345678901)];
    let bytes = layout.encode(&record)?;
    assert_eq!(bytes.len(), 34);
    assert_eq!(layout.decode(&bytes)?, record);

    let output = EncodeWriter::new(Vec::new(), Charset::Cp939);
    let mut writer = csv::Writer::from_writer(output);
    writer.write_record(["名前", "説明"])?;
    writer.write_record(["山田太郎", "引用\"と,区切り\n改行"])?;
    let bytes = writer
        .into_inner()
        .map_err(|error| error.into_error())?
        .finish()?;

    let input = DecodeReader::new(bytes.as_slice(), Charset::Cp939);
    let mut reader = csv::Reader::from_reader(input);
    let rows = reader.records().collect::<Result<Vec<_>, _>>()?;
    assert_eq!(rows[0].get(0), Some("山田太郎"));
    assert_eq!(rows[0].get(1), Some("引用\"と,区切り\n改行"));
    println!("JEF / Shift_JIS・固定長レコード・CP939 CSV の変換に成功しました。");
    Ok(())
}
