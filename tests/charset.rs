//! Mapping conformance, known vectors, malformed data and incremental conversion.

use jtc_utils::charset::*;

fn hex(value: &str) -> u32 {
    u32::from_str_radix(value, 16).unwrap()
}
fn unicode(value: &str) -> String {
    value
        .split('+')
        .map(|v| char::from_u32(hex(v)).unwrap())
        .collect()
}
fn encoded(code: u32, mixed: bool) -> Vec<u8> {
    if code <= 0xff {
        vec![code as u8]
    } else if mixed {
        vec![0x0e, (code >> 8) as u8, code as u8, 0x0f]
    } else {
        vec![(code >> 8) as u8, code as u8]
    }
}

macro_rules! mappings {
    ($name:literal, $charset:expr) => {
        (
            $charset,
            include_str!(concat!("../data/encode.", $name, ".csv")),
            include_str!(concat!("../data/decode.", $name, ".csv")),
        )
    };
}

#[test]
fn every_legacy_mapping_matches_source_data() {
    for (charset, encode, decode) in [
        mappings!("x-fujitsu-jef", JEF),
        mappings!("x-fujitsu-ebcdic-ascii", Charset::FujitsuEbcdicAscii),
        mappings!("x-fujitsu-ebcdic-kana", Charset::FujitsuEbcdicKana),
        mappings!("x-fujitsu-ebcdic-lower", Charset::FujitsuEbcdicLower),
        mappings!("cp942", Charset::Cp942),
        mappings!("cp942c", Charset::Cp942C),
        mappings!("cp943c", Charset::Cp943C),
        mappings!("cp8482", Charset::Cp930),
        mappings!("cp5123", Charset::Cp939),
        mappings!("cp11684", Charset::Cp930),
        mappings!("cp11684", Charset::Cp939),
    ] {
        let mixed = matches!(charset, Charset::Cp930 | Charset::Cp939);
        for line in encode.lines() {
            let parts: Vec<_> = line.splitn(3, ',').collect();
            let text = unicode(parts[0]);
            assert_eq!(
                charset.encode(&text).unwrap(),
                encoded(hex(parts[1]), mixed),
                "{} encode {}",
                charset.name(),
                parts[0]
            );
        }
        for line in decode.lines() {
            let parts: Vec<_> = line.splitn(3, ',').collect();
            assert_eq!(
                charset.decode(&encoded(hex(parts[0]), mixed)).unwrap(),
                unicode(parts[1]),
                "{} decode {}",
                charset.name(),
                parts[0]
            );
        }
    }
    for line in include_str!("../data/encode.jef-sequences.csv").lines() {
        let (text, code) = line.split_once(',').unwrap();
        assert_eq!(
            JEF.encode(&unicode(text)).unwrap(),
            encoded(hex(code), false)
        );
    }
}

#[test]
fn known_vectors_and_jef_private_use_range() {
    assert_eq!(
        SHIFT_JIS.encode("あいうｱｲｳ").unwrap(),
        [0x82, 0xa0, 0x82, 0xa2, 0x82, 0xa4, 0xb1, 0xb2, 0xb3]
    );
    assert_eq!(JEF.encode("あ").unwrap(), [0xa4, 0xa2]);
    assert_eq!(JEF.encode("\u{2000b}").unwrap(), [0x41, 0xa6]);
    assert_eq!(JEF.decode(&[0x41, 0xa6]).unwrap(), "\u{2000b}");
    for code in 0xe000..=0xec1d {
        let text = char::from_u32(code).unwrap().to_string();
        let bytes = JEF.encode(&text).unwrap();
        assert_eq!(
            bytes,
            [
                0x80 + ((code - 0xe000) / 94) as u8,
                0xa1 + ((code - 0xe000) % 94) as u8
            ]
        );
        assert_eq!(JEF.decode(&bytes).unwrap(), text);
    }
    assert_eq!(
        Charset::FujitsuEbcdicKana.encode("abc").unwrap(),
        [0xc1, 0xc2, 0xc3]
    );
    assert_eq!(
        Charset::FujitsuEbcdicKana
            .decode(&[0xc1, 0xc2, 0xc3])
            .unwrap(),
        "ABC"
    );
}

#[test]
fn standard_encodings_delegate_and_support_utf16_output() {
    for encoding in [
        encoding_rs::SHIFT_JIS,
        encoding_rs::EUC_JP,
        encoding_rs::ISO_2022_JP,
        encoding_rs::WINDOWS_1252,
    ] {
        let charset = Charset::from(encoding);
        let text = if encoding == encoding_rs::WINDOWS_1252 {
            "café"
        } else {
            "日本語ｶﾅ"
        };
        let (expected, _, errors) = encoding.encode(text);
        assert!(!errors);
        assert_eq!(charset.encode(text).unwrap(), expected.as_ref());
        let (expected, errors) = encoding.decode_without_bom_handling(&expected);
        assert!(!errors);
        assert_eq!(
            charset.decode(&charset.encode(text).unwrap()).unwrap(),
            expected
        );
    }
    let text = "Aあ😀";
    for (charset, expected) in [
        (UTF_16LE, vec![0x41, 0, 0x42, 0x30, 0x3d, 0xd8, 0, 0xde]),
        (UTF_16BE, vec![0, 0x41, 0x30, 0x42, 0xd8, 0x3d, 0xde, 0]),
    ] {
        assert_eq!(charset.encode(text).unwrap(), expected);
        assert_eq!(charset.decode(&expected).unwrap(), text);
    }
    assert!(matches!(
        Charset::Standard(encoding_rs::REPLACEMENT).encode("a"),
        Err(Error::UnsupportedEncoding(_))
    ));
}

#[test]
fn byte_splits_preserve_pending_input_and_shift_state() {
    for (charset, text) in [
        (JEF, "あ海\u{2000b}\u{1b019}\u{3099}\u{e000}"),
        (SHIFT_JIS, "AあいうｱｲｳB"),
        (EUC_JP, "AあいうB"),
        (UTF_8, "😀あA"),
        (UTF_16LE, "😀あA"),
        (UTF_16BE, "😀あA"),
        (Charset::Standard(encoding_rs::ISO_2022_JP), "Aあ海B"),
        (Charset::Cp930, "Aあ海B漢C"),
        (Charset::Cp939, "Aあ海B漢C"),
        (Charset::Cp942, "Aあ海B"),
        (Charset::Cp942C, "Aあ海B"),
        (Charset::Cp943C, "Aあ海B"),
    ] {
        let bytes = charset.encode(text).unwrap();
        let expected = charset.decode(&bytes).unwrap();
        for split in 0..=bytes.len() {
            let mut decoder = charset.decoder();
            let mut decoded = decoder.decode(&bytes[..split], false).unwrap();
            decoded.push_str(&decoder.decode(&[], false).unwrap());
            decoded.push_str(&decoder.decode(&bytes[split..], true).unwrap());
            assert_eq!(decoded, expected, "{} split {split}", charset.name());
        }
        let mut decoder = charset.decoder();
        let mut decoded = String::new();
        for byte in &bytes {
            decoded.push_str(&decoder.decode(&[*byte], false).unwrap());
        }
        decoded.push_str(&decoder.finish().unwrap());
        assert_eq!(decoded, expected);
        for split in text
            .char_indices()
            .map(|(index, _)| index)
            .chain([text.len()])
        {
            let mut encoder = charset.encoder();
            let mut encoded = encoder.encode(&text[..split], false).unwrap();
            encoded.extend(encoder.encode("", false).unwrap());
            encoded.extend(encoder.encode(&text[split..], true).unwrap());
            assert_eq!(encoded, bytes, "{} encode split {split}", charset.name());
        }
    }
}

#[test]
fn bom_handling_never_switches_charsets() {
    for charset in [UTF_8, UTF_16LE, UTF_16BE] {
        let bytes = charset.encode("\u{feff}あ").unwrap();
        assert_eq!(charset.decode(&bytes).unwrap(), "あ");
        let mut decoder = charset.decoder_with_options(DecodeOptions {
            bom: BomHandling::Preserve,
            ..Default::default()
        });
        assert_eq!(decoder.decode(&bytes, true).unwrap(), "\u{feff}あ");
        for split in 0..=bytes.len() {
            let mut decoder = charset.decoder();
            let mut output = decoder.decode(&bytes[..split], false).unwrap();
            output.push_str(&decoder.decode(&bytes[split..], true).unwrap());
            assert_eq!(output, "あ");
        }
    }
    assert!(UTF_8.decode(&[0xff, 0xfe, 0x41, 0]).is_err());
}

#[test]
fn malformed_strict_replacement_and_offsets() {
    for (charset, input, offset) in [
        (JEF, vec![0xa4], 0),
        (JEF, vec![0xa4, 0xa2, 0], 2),
        (SHIFT_JIS, vec![b'A', 0x82], 1),
        (Charset::Cp939, vec![0xc1, 0x0e, 0x44], 2),
        (UTF_8, vec![b'A', 0xff], 1),
    ] {
        assert!(
            matches!(charset.decode(&input), Err(Error::MalformedInput { offset: actual, .. }) if offset == actual)
        );
        let mut decoder = charset.decoder_with_options(DecodeOptions {
            errors: ErrorPolicy::Replace,
            ..Default::default()
        });
        assert!(decoder.decode(&input, true).unwrap().ends_with('\u{fffd}'));
    }
    let mut decoder = SHIFT_JIS.decoder();
    assert_eq!(decoder.decode(&[b'A', 0x82], false).unwrap(), "A");
    assert!(matches!(
        decoder.finish(),
        Err(Error::MalformedInput { offset: 1, .. })
    ));
    assert!(matches!(decoder.finish(), Err(Error::Finished)));
    for charset in [
        SHIFT_JIS,
        EUC_JP,
        JEF,
        Charset::Cp930,
        Charset::FujitsuEbcdicKana,
    ] {
        assert!(!charset.can_encode("😀"));
        assert!(matches!(
            charset.encode("あ😀"),
            Err(Error::Unmappable { .. })
        ));
        let mut encoder = charset.encoder_with_options(EncodeOptions {
            errors: ErrorPolicy::Replace,
            ..Default::default()
        });
        let bytes = encoder.encode("😀", true).unwrap();
        assert_eq!(
            charset.decode(&bytes).unwrap(),
            if charset == JEF { "　" } else { "?" }
        );
    }
    let mut encoder = SHIFT_JIS.encoder();
    encoder.encode("あ", false).unwrap();
    assert!(matches!(
        encoder.encode("a😀", true),
        Err(Error::Unmappable { offset: 4, .. })
    ));
    let mut encoder =
        Charset::Standard(encoding_rs::ISO_2022_JP).encoder_with_options(EncodeOptions {
            errors: ErrorPolicy::Replace,
            ..Default::default()
        });
    let bytes = encoder.encode("漢😀漢", true).unwrap();
    assert_eq!(
        Charset::Standard(encoding_rs::ISO_2022_JP)
            .decode(&bytes)
            .unwrap(),
        "漢?漢"
    );
}

#[test]
fn shifted_fields_and_labels() {
    for charset in [Charset::Cp930, Charset::Cp939] {
        let normal = charset.encode("日本").unwrap();
        let bytes = charset
            .encoder_with_options(EncodeOptions {
                initial_shift: true,
                ..Default::default()
            })
            .encode("日本", true)
            .unwrap();
        assert_eq!(bytes, normal[1..normal.len() - 1]);
        let decoded = charset
            .decoder_with_options(DecodeOptions {
                initial_shift: true,
                ..Default::default()
            })
            .decode(&bytes, true)
            .unwrap();
        assert_eq!(decoded, "日本");
    }
    for label in [" Shift_JIS\n", "Windows-31J", "CP932"] {
        assert_eq!(Charset::for_label(label), Some(SHIFT_JIS));
    }
    assert_eq!(Charset::for_label("JEF"), Some(JEF));
    assert_eq!(Charset::for_label("ibm-939"), Some(Charset::Cp939));
    assert!(Charset::for_label("unknown").is_none());
}

#[test]
fn replacement_uses_ascii_question_mark_in_mixed_ibm_streams() {
    for charset in [Charset::Cp930, Charset::Cp939] {
        let mut encoder = charset.encoder_with_options(EncodeOptions {
            errors: ErrorPolicy::Replace,
            ..Default::default()
        });
        let bytes = encoder.encode("漢😀漢", true).unwrap();
        assert_eq!(charset.decode(&bytes).unwrap(), "漢?漢");
    }
}
