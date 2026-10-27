//! Japanese business text conventions beyond generic Unicode/kana utilities.

use crate::table;
use std::{error, fmt};
use unicode_normalization::UnicodeNormalization;

/// An empty bank name or a character outside this library's Zengin repertoire.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ZenginError {
    /// Empty strings are not valid names.
    Empty,
    /// A character remains unrepresentable after conversion.
    InvalidCharacter {
        /// Invalid character.
        character: char,
        /// UTF-8 byte offset in the converted string.
        offset: usize,
    },
}

impl fmt::Display for ZenginError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Empty => f.write_str("Zengin name is empty"),
            Self::InvalidCharacter { character, offset } => {
                write!(f, "invalid Zengin character {character:?} at byte {offset}")
            }
        }
    }
}
impl error::Error for ZenginError {}

fn is_zengin_char(character: char) -> bool {
    matches!(character, ' ' | '0'..='9' | 'A'..='Z' | '(' | ')' | '/' | '.' | '\\' | '-' | '\u{ff62}' | '\u{ff63}' | '\u{ff71}'..='\u{ff9f}')
}

/// Check the nonempty repertoire used by jtc-utils-js: uppercase ASCII letters,
/// digits, space, `()/.-\`, halfwidth corner brackets and large halfwidth kana.
/// This does not validate a particular bank's field length or additional rules.
pub fn is_zengin_kana(input: &str) -> bool {
    !input.is_empty() && input.chars().all(is_zengin_char)
}

/// Convert hiragana/fullwidth kana, Latin letters, small kana, wo, prolonged
/// sound marks and punctuation to the Zengin repertoire. Unsupported characters
/// are preserved, so call [`zengin_kana`] when the result must be valid.
/// Unlike the JS implementation, ASCII lowercase and existing halfwidth small
/// kana are also normalized. Kanji are never transliterated.
///
/// ```
/// use jtc_utils::text::to_zengin_kana;
/// assert_eq!(to_zengin_kana("カ）ベローチェ"), "ｶ)ﾍﾞﾛ-ﾁｴ");
/// assert_eq!(to_zengin_kana("abc ｧｨｩｪｫｯｬｭｮｦｰ"), "ABC ｱｲｳｴｵﾂﾔﾕﾖｵ-");
/// ```
pub fn to_zengin_kana(input: &str) -> String {
    let mut output = String::with_capacity(input.len());
    for character in input.chars() {
        let extra = match character {
            'a'..='z' => Some(character.to_ascii_uppercase()),
            'ｧ' => Some('ｱ'),
            'ｨ' => Some('ｲ'),
            'ｩ' => Some('ｳ'),
            'ｪ' => Some('ｴ'),
            'ｫ' => Some('ｵ'),
            'ｯ' => Some('ﾂ'),
            'ｬ' => Some('ﾔ'),
            'ｭ' => Some('ﾕ'),
            'ｮ' => Some('ﾖ'),
            'ｦ' => Some('ｵ'),
            'ｰ' => Some('-'),
            '｡' | '､' | '･' => Some('.'),
            'ヰ' => Some('ｲ'),
            'ヱ' => Some('ｴ'),
            'ヮ' => Some('ﾜ'),
            'ヵ' => Some('ｶ'),
            'ヶ' => Some('ｹ'),
            _ => None,
        };
        if let Some(mapped) = extra {
            output.push(mapped);
        } else if let Some((first, second)) = table::MAP_ZENGIN.get(character as u32, 0) {
            output.push(char::from_u32(first).unwrap());
            if second != 0 {
                output.push(char::from_u32(second).unwrap());
            }
        } else {
            output.push(character);
        }
    }
    output
}

/// Convert and validate a nonempty Zengin name, returning any unsupported scalar.
pub fn zengin_kana(input: &str) -> Result<String, ZenginError> {
    let output = to_zengin_kana(input);
    if output.is_empty() {
        return Err(ZenginError::Empty);
    }
    if let Some((offset, character)) = output.char_indices().find(|&(_, c)| !is_zengin_char(c)) {
        return Err(ZenginError::InvalidCharacter { character, offset });
    }
    Ok(output)
}

/// NFC-normalize text while preserving the compatibility ideographs excluded
/// by jtc-utils-js (important for Japanese names). CRLF and CR become LF.
/// Unicode normalization itself is delegated to `unicode-normalization`.
/// No width folding or unsafe-character filtering is performed.
///
/// ```
/// use jtc_utils::text::normalize_japanese;
/// assert_eq!(normalize_japanese("神か\u{3099}\r\n"), "神が\n");
/// ```
pub fn normalize_japanese(input: &str) -> String {
    let mut output = String::with_capacity(input.len());
    let mut start = 0;
    let mut chars = input.char_indices().peekable();
    while let Some((index, character)) = chars.next() {
        if character == '\r' || preserve_ideograph(character) {
            output.extend(input[start..index].nfc());
            if character == '\r' {
                output.push('\n');
                start = index + 1;
                if let Some(&(_, '\n')) = chars.peek() {
                    chars.next();
                    start += 1;
                }
            } else {
                output.push(character);
                start = index + character.len_utf8();
            }
        }
    }
    output.extend(input[start..].nfc());
    output
}

fn preserve_ideograph(character: char) -> bool {
    matches!(character,
        '\u{f91d}' | '\u{f928}' | '\u{f929}' | '\u{f936}' | '\u{f970}' | '\u{f9d0}' | '\u{f9dc}' |
        '\u{fa10}' | '\u{fa12}' | '\u{fa15}'..='\u{fa1e}' | '\u{fa20}' | '\u{fa22}' |
        '\u{fa25}' | '\u{fa26}' | '\u{fa2a}'..='\u{fa2d}' | '\u{fa30}'..='\u{fa6d}'
    )
}
