# 変換表

`jtc-utils-js/data/` を基にした文字コードの対応表です。標準文字コードの表は
含めず、UTF-8・Shift_JIS・EUC-JP などは `encoding_rs` に任せます。

- `encode.*.csv`: Unicode → 文字コード。第1列が Unicode、第2列がコード。
- `decode.*.csv`: 文字コード → Unicode。第1列がコード、第2列が Unicode。
- `cp8482` は CP930 の1バイト部、`cp5123` は CP939 の1バイト部、
  `cp11684` は両者の IBM 漢字部です。
- `encode.jef-sequences.csv` は JEF の変体仮名＋結合濁点の優先マッピングです。
- `map.zengin.csv` は全銀カナへの変換表です。元データのヘッダーと説明列を
  除き、連結された Unicode 値を `+` 区切りにしています。

数値はすべて16進数です。複数の Unicode スカラーは `1b019+3099` のように
記載します。文字コードと Unicode の対応には非可逆な別名があるため、encode
と decode の表を別々に保持しています。JEF の外字領域 E000–EC1D は規則的に
計算するため、表には追加していません。

`build.rs` が先頭2列だけを読み、検証済みのソート済みバイナリ表を `OUT_DIR`
に生成します。実行時の CSV 読み込み、初期化、JS リポジトリへの依存はありません。
対応表を更新した場合は `cargo test --test charset --test text` で照合できます。
