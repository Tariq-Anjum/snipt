//! JavaScript string semantics needed for parity with the TS oracle.
//!
//! * `String.prototype.trim()` strips the ECMAScript WhiteSpace + LineTerminator
//!   set, which includes U+FEFF and excludes U+0085 — unlike `str::trim`.
//! * `String.prototype.length` counts UTF-16 code units.

/// ECMAScript `WhiteSpace` ∪ `LineTerminator` (ES2024 §12.2, §12.3).
pub fn is_js_whitespace(c: char) -> bool {
    matches!(
        c,
        '\u{0009}'
            | '\u{000A}'
            | '\u{000B}'
            | '\u{000C}'
            | '\u{000D}'
            | '\u{0020}'
            | '\u{00A0}'
            | '\u{1680}'
            | '\u{2000}'
            ..='\u{200A}'
                | '\u{2028}'
                | '\u{2029}'
                | '\u{202F}'
                | '\u{205F}'
                | '\u{3000}'
                | '\u{FEFF}'
    )
}

/// Regex character-class body (without brackets) equal to JS `\s`.
pub const JS_WS_CLASS: &str = r"\t\n\x0B\x0C\r \u{A0}\u{1680}\u{2000}-\u{200A}\u{2028}\u{2029}\u{202F}\u{205F}\u{3000}\u{FEFF}";

/// JS `String.prototype.trim()`.
pub fn trim(s: &str) -> &str {
    s.trim_matches(is_js_whitespace)
}

/// JS `String.prototype.length` (UTF-16 code units).
pub fn utf16_len(s: &str) -> usize {
    s.chars().map(char::len_utf16).sum()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn trim_matches_js_set() {
        assert_eq!(trim("\u{FEFF} pass \u{3000}"), "pass");
        // U+0085 (NEL) is White_Space in Unicode but not in ECMAScript.
        assert_eq!(trim("\u{0085}x"), "\u{0085}x");
        assert_eq!(utf16_len("a₹😀"), 4);
    }
}
