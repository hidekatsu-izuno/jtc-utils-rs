//! Japanese business text conventions and name preservation.

use jtc_utils::text::*;

#[test]
fn zengin_reference_table_and_bank_name_vectors() {
    for line in include_str!("../data/map.zengin.csv").lines() {
        let (from, to) = line.split_once(',').unwrap();
        let input = char::from_u32(u32::from_str_radix(from, 16).unwrap())
            .unwrap()
            .to_string();
        let expected: String = to
            .split('+')
            .map(|v| char::from_u32(u32::from_str_radix(v, 16).unwrap()).unwrap())
            .collect();
        assert_eq!(to_zengin_kana(&input), expected, "{from}");
    }
    for (input, expected) in [
        ("タグチ　トモロヲ", "ﾀｸﾞﾁ ﾄﾓﾛｵ"),
        ("カ）ベローチェ", "ｶ)ﾍﾞﾛ-ﾁｴ"),
        ("アガサ・クリスティー", "ｱｶﾞｻ.ｸﾘｽﾃｲ-"),
        ("abc ｧｨｩｪｫｯｬｭｮｦｰ･｡､", "ABC ｱｲｳｴｵﾂﾔﾕﾖｵ-..."),
        ("ヮヰヱヵヶ", "ﾜｲｴｶｹ"),
        ("か\u{3099}は\u{309a}", "ｶﾞﾊﾟ"),
        ("¥￥\\［］｛｝「」", "\\\\\\()()｢｣"),
    ] {
        assert_eq!(zengin_kana(input).unwrap(), expected);
        assert!(is_zengin_kana(expected));
    }
    assert!(!is_zengin_kana(""));
    assert!(!is_zengin_kana("ｧ"));
    assert!(!is_zengin_kana("abc"));
    assert!(matches!(zengin_kana(""), Err(ZenginError::Empty)));
    assert_eq!(to_zengin_kana("山田😀"), "山田😀");
    assert!(matches!(
        zengin_kana("ア山"),
        Err(ZenginError::InvalidCharacter {
            character: '山',
            offset: 3
        })
    ));
}

#[test]
fn normalization_preserves_japanese_name_forms_and_line_boundaries() {
    assert_eq!(normalize_japanese("Aあｱ亜欄か\u{3099}\r\n"), "Aあｱ亜欄が\n");
    assert_eq!(normalize_japanese("神﨑羽都塚"), "神﨑羽都塚");
    assert_eq!(
        normalize_japanese("e\u{301}か\u{3099}は\u{309a}ヲ\u{3099}"),
        "éがぱヺ"
    );
    for (input, expected) in [
        ("\r", "\n"),
        ("\r\n", "\n"),
        ("\r\r\n", "\n\n"),
        ("x\r\n\ry\r", "x\n\ny\n"),
        ("", ""),
        ("😀\u{e000}\0", "😀\u{e000}\0"),
    ] {
        assert_eq!(normalize_japanese(input), expected);
        assert_eq!(normalize_japanese(&normalize_japanese(input)), expected);
    }
}
