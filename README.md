# jtc-utils-rs

日本企業のレガシーデータを扱うための Rust ライブラリです。
`jtc-utils-js` の要件を基に、Rust の型、`Result`、`std::io` に合わせて実装しています。
ライブラリ crate 名は `jtc_utils`、パッケージ名は `jtc-utils` です。

## 導入

Rust 1.85 以降が必要です。ローカルのアプリケーションから利用する場合:

```toml
[dependencies]
jtc-utils = { path = "../jtc-utils-rs" }
```

実行時の依存は `encoding_rs` と `unicode-normalization` です。
Node.js や `jtc-utils-js` のチェックアウトはビルド・実行ともに不要です。

## 提供する機能

| モジュール | 用途 |
| --- | --- |
| `charset` | `encoding_rs` と JEF・富士通 EBCDIC・IBM コードの共通エンコード／デコード API |
| `io` | エンコードされた入出力を UTF-8 の `Read` / `Write` に接続 |
| `decimal` | packed / zoned decimal の検証と変換。値は正確な `i128` |
| `fixed` | フィールドごとの文字コードと decimal 形式を持つバイト固定長レコード |
| `text` | 全銀カナへの変換・検証、氏名の互換漢字を保持する正規化 |

既存 crate と重なる汎用機能は実装していません。

| JS 版の機能 | Rust での利用先 |
| --- | --- |
| CSV の構文解析・引用符・改行処理 | [`csv`](https://docs.rs/csv/) と本ライブラリの `io` |
| 日付の解析・書式化、タイムゾーン | [`chrono`](https://docs.rs/chrono/)、[`time`](https://docs.rs/time/)、[`chrono-tz`](https://docs.rs/chrono-tz/) |
| 和暦、ロケール依存の日付・数値表示 | [`icu_calendar`](https://docs.rs/icu_calendar/)、[`icu_datetime`](https://docs.rs/icu_datetime/)、[`icu_decimal`](https://docs.rs/icu_decimal/) |
| 数値の解析・整形、十進数演算 | 標準の `FromStr` / `format!`、[`rust_decimal`](https://docs.rs/rust_decimal/) |
| 汎用の全角・半角・ひらがな・カタカナ変換や判定 | [`kanaria`](https://docs.rs/kanaria/)、[`wana_kana`](https://docs.rs/wana_kana/) |
| NFC / NFKC などの Unicode 正規化 | [`unicode-normalization`](https://docs.rs/unicode-normalization/) |
| URL・メールアドレス・電話番号の検証 | [`url`](https://docs.rs/url/)、[`validator`](https://docs.rs/validator/)、[`phonenumber`](https://docs.rs/phonenumber/) |
| メモリストリーム・バイナリ整数・浮動小数点 | `std::io::Cursor`、`Vec<u8>`、標準の `from_le_bytes` / `to_be_bytes` など |
| BMP 判定・プラットフォーム判定 | `str::chars`、`cfg!` / `std::env::consts` |

`isWebSafeString` 相当の汎用安全性判定は提供しません。出力先に応じた
エスケープや入力要件は、アプリケーション側で決めてください。

## 文字コード

JEF と Shift_JIS を同じ API で扱えます。

```rust
use jtc_utils::charset::{JEF, SHIFT_JIS};

for charset in [JEF, SHIFT_JIS] {
    let bytes = charset.encode("日本語")?;
    assert_eq!(charset.decode(&bytes)?, "日本語");
}
# Ok::<(), jtc_utils::charset::Error>(())
```

| 文字コード | 指定方法 |
| --- | --- |
| UTF-8 / UTF-16LE / UTF-16BE | `charset::UTF_8` / `UTF_16LE` / `UTF_16BE` |
| Shift_JIS / Windows-31J | `charset::SHIFT_JIS` / `WINDOWS_31J`（同じ値） |
| EUC-JP | `charset::EUC_JP` |
| その他 `encoding_rs` の文字コード | `Charset::Standard(encoding_rs::ISO_2022_JP)` など、または `Charset::for_label(...)` |
| 富士通 JEF 漢字 | `charset::JEF` / `Charset::Jef` |
| 富士通 EBCDIC ASCII / カナ / 英小文字 | `Charset::FujitsuEbcdicAscii` / `FujitsuEbcdicKana` / `FujitsuEbcdicLower` |
| IBM EBCDIC + 漢字 | `Charset::Cp930` / `Cp939` |
| IBM 系 Shift_JIS 変種 | `Charset::Cp942` / `Cp942C` / `Cp943C` |

`Charset::for_label("shift_jis")`、`Charset::for_label("jef")` などでも選べます。
Windows-31J と CP943C のようにマッピングが違う文字コードは区別しています。

標準文字コードの変換は [`encoding_rs`](https://docs.rs/encoding_rs/) に委譲します。
`encoding_rs` がエンコードを提供していない UTF-16LE/BE は、標準の
`str::encode_utf16` とバイト順変換で補います。replacement 疑似エンコーディングの
エンコードはエラーになります。

既定では不正なバイト列と変換不能文字を `Result::Err` にします。
`can_encode` で表現可能性を検査できます。既存コードには非可逆な別名があるため、
表現可能であっても元の Unicode と完全に同一に戻るとは限りません。

置換を許す場合は明示的に指定します。デコード時は U+FFFD、エンコード時は `?`
に置換します。JEF は ASCII を持たないため全角空白 `0x4040` になります。
`encoding_rs` の HTML 数値文字参照への置換は使いません。

```rust
use jtc_utils::charset::{EncodeOptions, ErrorPolicy, SHIFT_JIS};

let mut encoder = SHIFT_JIS.encoder_with_options(EncodeOptions {
    errors: ErrorPolicy::Replace,
    ..Default::default()
});
assert_eq!(encoder.encode("あ😀", true)?, [0x82, 0xa0, b'?']);
# Ok::<(), jtc_utils::charset::Error>(())
```

JEF は **2バイトの漢字コードだけ**を扱います。富士通 EBCDIC との混在データは
フィールドごとに文字コードを選択してください。補助面の漢字・変体仮名と
結合濁点、外字 E000–EC1D に対応しています。外字の字形自体は定義しません。

### ストリームと BOM

`charset.encoder()` / `charset.decoder()` は1ストリームに対応します。
`encode(chunk, false)` / `decode(chunk, false)` で途中のチャンクを渡し、
最後だけ `true`、または `finish()` で確定します。終了後・エラー後は新しい
変換器を作ってください。デコードは多バイト文字の途中での分割、エンコードは
JEF の変体仮名と濁点の間での分割も扱えます。

CP930/CP939 は SO/SI の状態を保持します。既定ではエンコード終了時に SI を
出力します。SO なしで漢字から始まるフィールドには `initial_shift: true` を
指定できます。この指定ではエンコード末尾の SI は自動追加しません。

エンコードは BOM を自動追加しません。デコードは既定で選択した Unicode
エンコーディングに一致する先頭 BOM だけを除去し、別の文字コードへ自動変更
しません。保持する場合は `DecodeOptions { bom: BomHandling::Preserve, .. }`
を指定します。固定長フィールド内では常に BOM を通常の文字として扱います。

## CSV との組み合わせ

アプリケーション側で `csv = "1.4"` を依存に追加します。
CSV の構文処理はすべて `csv` crate が担当します。

```rust
use jtc_utils::charset::SHIFT_JIS;
use jtc_utils::io::{DecodeReader, EncodeWriter};

let output = EncodeWriter::new(Vec::new(), SHIFT_JIS);
let mut writer = csv::Writer::from_writer(output);
writer.write_record(["名前", "説明"])?;
writer.write_record(["山田太郎", "引用\"と,区切り\n改行"])?;
let encoded = writer.into_inner().map_err(|error| error.into_error())?.finish()?;

let input = DecodeReader::new(encoded.as_slice(), SHIFT_JIS);
let mut reader = csv::Reader::from_reader(input);
for row in reader.records() {
    let row = row?;
    assert_eq!(&row[0], "山田太郎");
}
# Ok::<(), Box<dyn std::error::Error>>(())
```

`Vec<u8>` の代わりに `File`、`BufReader` / `BufWriter` なども渡せます。
`EncodeWriter` は入力 UTF-8 の途中での分割も処理します。`flush()` は
ストリームを終了しないため、**最後に必ず `finish()` を呼んでください**。
`Drop` は確定処理をしません。変換エラーは `io::ErrorKind::InvalidData` で返します。

## 固定長レコード

長さはすべて **エンコード後のバイト数**です。フィールドごとに文字コードを
選べるため、EBCDIC・JEF・packed decimal が混在するレコードを扱えます。

```rust
use jtc_utils::charset::JEF;
use jtc_utils::decimal::PackedSign;
use jtc_utils::fixed::{Field, Layout, TextField, Trim, Value};

let mut name = TextField::new(12, JEF);
name.trim = Trim::End;
let layout = Layout::new(vec![
    Field::Text(name),
    Field::Packed { width: 6, sign: PackedSign::Signed },
])?;

let record = vec!["山田太郎".into(), Value::Decimal(12345678901)];
let bytes = layout.encode(&record)?;
assert_eq!(bytes.len(), 18);
assert_eq!(layout.decode(&bytes)?, record);
# Ok::<(), jtc_utils::fixed::Error>(())
```

`TextField` は左右の配置、埋め文字、読み取り時の空白除去を設定できます。
埋め文字の既定は半角空白、JEF の場合は全角空白です。文字が収まらない場合、
または残りのバイト数を埋め文字で満たせない場合はエラーにします。
文字の途中で切り捨てたり、数値の上位桁を捨てたりする動作はしません。

`Field::Zoned` では ASCII / EBCDIC の zone と符号位置を指定できます。
`Field::Bytes` は予約領域や、標準の `from_le_bytes` などで読む整数・浮動小数点に
使えます。動的なレコード形式では、アプリケーション側で種別を読んで `Layout`
を選び、対象のバイト列に `decode` を呼び出してください。

ファイルを順次処理する場合は `fixed::Reader` / `fixed::Writer` を使います。
`Reader` は `Iterator<Item = Result<Vec<Value>, fixed::Error>>` です。
既定ではレコードを直接連結し、必要なら `.with_separator(b"\r\n".to_vec())`
で各レコード末尾のバイト列を指定します。最後のレコードにも同じ区切りが必要です。
途中で終わるレコードや不一致の区切りはエラー、境界での EOF だけを正常終了とします。
件数は `record_count()`、出力の確定は `Writer::finish()` で取得できます。

### packed / zoned decimal

`decimal::encode_packed` / `decode_packed`、`encode_zoned` / `decode_zoned` は
固定長レコードとは独立して利用できます。

| JS 版の型 | Rust の設定 |
| --- | --- |
| `packed` | `PackedSign::Signed`。出力符号は正 C / 負 D |
| `upacked` | `PackedSign::Unsigned`。末尾 F の符号ニブルあり |
| `npacked` | `PackedSign::Signless`。符号ニブルなし |
| `zoned` | `ZonedSign::Overpunch` |
| `uzoned` | `ZonedSign::Unsigned` |
| `lzoned` / `tzoned` | `ZonedSign::Leading` / `Trailing` |

ASCII の overpunch は正 3 / 負 7、EBCDIC は出力に正 C / 負 D を使います。
EBCDIC の通常の数字は F zone です。読み取り時は数字だけでなく zone と
符号も検証します。値は `i128` の整数係数として返し、暗黙の小数桁は
アプリケーション側のスキーマや `rust_decimal` などで適用してください。
`f64` を経由しないため、2^53 を超える値も正確に扱います。

## 日本語の業務文字列

```rust
use jtc_utils::text::{is_zengin_kana, normalize_japanese, to_zengin_kana, zengin_kana};

assert_eq!(to_zengin_kana("カ）ベローチェ"), "ｶ)ﾍﾞﾛ-ﾁｴ");
assert_eq!(zengin_kana("abc ｧｦｰ")?, "ABC ｱｵ-");
assert!(is_zengin_kana("ﾔﾏﾀﾞ ﾀﾛｳ"));
assert!(zengin_kana("山田太郎").is_err());
assert_eq!(normalize_japanese("神か\u{3099}\r\n"), "神が\n");
# Ok::<(), jtc_utils::text::ZenginError>(())
```

`to_zengin_kana` は変換不能文字を保持します。`zengin_kana` は変換後の文字集合も
検証し、漢字などが残っていればエラーにします。漢字の読みは推測しません。
JS 版に加え、ASCII 小文字、既存の半角小書きカナ・ヲ・長音記号も変換します。
空文字列は全銀カナとして認めません。文字集合は JS 版の定義を基にしており、
個別の銀行の記号制限やフィールド長の検証はアプリケーション側で行ってください。

`normalize_japanese` は JS 版で除外している氏名用の互換漢字を保持し、それ以外を
`unicode-normalization` の NFC で正規化します。CRLF / CR は LF に揃えます。
全角半角の変換や文字の削除はしません。

## 開発・検証

```sh
cargo fmt --all -- --check
cargo clippy --all-targets -- -D warnings
cargo test
cargo run --example legacy_records
cargo doc --no-deps
```

変換表の全件照合、JEF の補助面・結合文字・外字、チャンク境界、SO/SI、BOM、
変換エラー、CSV 連携、固定長レコードの欠損、decimal の `i128` 境界値を検証します。
変換表の構造は [data/README.md](data/README.md) を参照してください。

License: [MIT](./LICENSE)


<!-- 
## Publish

```sh
1. Update to vX.X.X on cargo.toml and README.md.

2. Run commnads
cargo test
cargo publish --dry-run
git tag vX.X.X
git push origin --tags
cargo publish
```
-->