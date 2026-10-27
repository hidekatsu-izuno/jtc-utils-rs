//! Read original jtc-utils-js fixtures without changing their wire format.

use jtc_utils::{
    charset::{Charset, SHIFT_JIS},
    decimal::*,
    fixed::{Field, Layout, Reader, TextField, Trim, Value},
};

#[test]
fn original_cp939_mainframe_fixture() {
    let text = || {
        let mut field = TextField::new(4, Charset::Cp939);
        field.trim = Trim::Both;
        Field::Text(field)
    };
    let schema = Layout::new(vec![
        text(),
        text(),
        Field::Bytes(4),
        Field::Bytes(4),
        Field::Bytes(4),
        Field::Bytes(4),
        Field::Zoned {
            width: 4,
            zone: Zone::Ebcdic,
            sign: ZonedSign::Overpunch,
        },
        Field::Zoned {
            width: 4,
            zone: Zone::Ebcdic,
            sign: ZonedSign::Unsigned,
        },
        Field::Packed {
            width: 4,
            sign: PackedSign::Signed,
        },
        Field::Packed {
            width: 4,
            sign: PackedSign::Signless,
        },
    ])
    .unwrap();
    let bytes = include_bytes!("fixtures/FixlenReader.cp939.txt");
    let records = Reader::new(bytes.as_slice(), schema)
        .with_separator(vec![0x15])
        .collect::<Result<Vec<_>, _>>()
        .unwrap();
    assert_eq!(records.len(), 5);
    for (row, expected) in records.iter().zip([0i32, 1, -2, 341, -341]) {
        for column in [0, 1] {
            let Value::Text(text) = &row[column] else {
                panic!("text field")
            };
            assert_eq!(text.parse::<i32>().unwrap(), expected);
        }
        for (column, little, signed) in [
            (2, true, true),
            (3, false, true),
            (4, true, false),
            (5, false, false),
        ] {
            let Value::Bytes(bytes) = &row[column] else {
                panic!("binary field")
            };
            let array = bytes.as_slice().try_into().unwrap();
            let actual = if little {
                u32::from_le_bytes(array)
            } else {
                u32::from_be_bytes(array)
            };
            if signed {
                assert_eq!(actual as i32, expected);
            } else {
                assert_eq!(actual, expected as u32);
            }
        }
        assert_eq!(row[6], Value::Decimal(i128::from(expected)));
        assert_eq!(row[7], Value::Decimal(i128::from(expected.abs())));
        assert_eq!(row[8], Value::Decimal(i128::from(expected)));
        assert_eq!(row[9], Value::Decimal(i128::from(expected.abs())));
    }
}

#[test]
fn original_shift_jis_fixture_with_record_type_selection() {
    let bytes = include_bytes!("fixtures/FixlenReader.windows-31j.txt");
    let mut rows = Vec::new();
    for raw in bytes.chunks_exact(18) {
        assert_eq!(&raw[16..], b"\r\n");
        let fields = if raw[0] == b'1' {
            vec![
                Field::Bytes(1),
                Field::Text(TextField::new(10, SHIFT_JIS)),
                Field::Bytes(5),
            ]
        } else {
            vec![
                Field::Bytes(1),
                Field::Text(TextField::new(5, SHIFT_JIS)),
                Field::Text(TextField::new(5, SHIFT_JIS)),
                Field::Text(TextField::new(5, SHIFT_JIS)),
            ]
        };
        let values = Layout::new(fields).unwrap().decode(&raw[..16]).unwrap();
        rows.push(
            values
                .into_iter()
                .filter_map(|v| {
                    if let Value::Text(text) = v {
                        Some(text)
                    } else {
                        None
                    }
                })
                .collect::<Vec<_>>(),
        );
    }
    assert_eq!(
        rows,
        vec![vec!["あいうえお"], vec!["ABCDE", "abcde", "01234"]]
    );
}
