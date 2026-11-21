//! Translate the (non-`u`) JavaScript regex literals of the TS engine into
//! `regex` crate syntax with JS semantics:
//!
//! * `\b`/`\B` → ASCII word boundary `(?-u:\b)` (JS `\b` is ASCII-only).
//! * `\d`, `\w` → ASCII classes; `\s` → the exact ECMAScript whitespace set.
//! * `/i` → ASCII-only case folding by expanding letters (`k` → `[kK]`), because
//!   Rust's Unicode `(?i)` would also match U+212A KELVIN SIGN and U+017F LONG S,
//!   which JS non-unicode `/i` never folds onto ASCII.
//! * `\uXXXX` → `\u{XXXX}`.
//!
//! Only constructs used by src/engine are supported; anything else panics at
//! first use (patterns are constants, so this is caught by every test run).

use regex::Regex;
use sms_types::js::JS_WS_CLASS;

const WORD: &str = "A-Za-z0-9_";

pub fn translate(js: &str, ignore_case: bool) -> String {
    let chars: Vec<char> = js.chars().collect();
    let mut out = String::with_capacity(js.len() * 2);
    let mut in_class = false;
    let mut i = 0;
    while i < chars.len() {
        let c = chars[i];
        if c == '\\' {
            let e = *chars.get(i + 1).expect("dangling escape");
            i += 2;
            match (e, in_class) {
                ('b', false) => out.push_str(r"(?-u:\b)"),
                ('B', false) => out.push_str(r"(?-u:\B)"),
                ('d', false) => out.push_str("[0-9]"),
                ('d', true) => out.push_str("0-9"),
                ('D', false) => out.push_str("[^0-9]"),
                ('w', false) => out.push_str(&format!("[{WORD}]")),
                ('w', true) => out.push_str(WORD),
                ('W', false) => out.push_str(&format!("[^{WORD}]")),
                ('s', false) => out.push_str(&format!("[{JS_WS_CLASS}]")),
                ('s', true) => out.push_str(JS_WS_CLASS),
                ('S', false) => out.push_str(&format!("[^{JS_WS_CLASS}]")),
                ('u', _) => {
                    let hex: String = chars[i..i + 4].iter().collect();
                    i += 4;
                    out.push_str(&format!("\\u{{{hex}}}"));
                }
                (e, _) if e.is_ascii_alphanumeric() => {
                    panic!("jsre: unsupported escape \\{e} in {js}")
                }
                (e, _) => out.push_str(&regex::escape(&e.to_string())),
            }
            continue;
        }
        if in_class {
            match c {
                ']' => {
                    in_class = false;
                    out.push(']');
                }
                c if ignore_case && c.is_ascii_alphabetic() => {
                    let is_range = chars.get(i + 1) == Some(&'-')
                        && chars.get(i + 2).is_some_and(|d| d.is_ascii_alphabetic());
                    if is_range {
                        let d = chars[i + 2];
                        out.push(c);
                        out.push('-');
                        out.push(d);
                        out.push(swap_case(c));
                        out.push('-');
                        out.push(swap_case(d));
                        i += 3;
                        continue;
                    }
                    out.push(c);
                    out.push(swap_case(c));
                }
                '&' | '~' | '[' => {
                    out.push('\\');
                    out.push(c);
                }
                c => out.push(c),
            }
            i += 1;
            continue;
        }
        match c {
            '[' => {
                in_class = true;
                out.push('[');
                if chars.get(i + 1) == Some(&'^') {
                    out.push('^');
                    i += 1;
                }
            }
            '.' => out.push_str(r"[^\n\r\u{2028}\u{2029}]"),
            c if ignore_case && c.is_ascii_alphabetic() => {
                out.push('[');
                out.push(c);
                out.push(swap_case(c));
                out.push(']');
            }
            '#' | '&' | '~' | ' ' => out.push_str(&regex::escape(&c.to_string())),
            c => out.push(c),
        }
        i += 1;
    }
    assert!(!in_class, "jsre: unterminated class in {js}");
    out
}

fn swap_case(c: char) -> char {
    if c.is_ascii_lowercase() {
        c.to_ascii_uppercase()
    } else {
        c.to_ascii_lowercase()
    }
}

/// Compile a JS regex literal body with JS semantics.
pub fn compile(js: &str, ignore_case: bool) -> Regex {
    let pat = translate(js, ignore_case);
    Regex::new(&pat).unwrap_or_else(|e| panic!("jsre: {js} -> {pat}: {e}"))
}

/// Lazily compiled JS regex constant.
macro_rules! js_regex {
    ($name:ident, $pat:expr) => {
        js_regex!($name, $pat, false);
    };
    ($name:ident, $pat:expr, i) => {
        js_regex!($name, $pat, true);
    };
    ($name:ident, $pat:expr, $ci:expr) => {
        fn $name() -> &'static regex::Regex {
            static RE: std::sync::OnceLock<regex::Regex> = std::sync::OnceLock::new();
            RE.get_or_init(|| $crate::jsre::compile($pat, $ci))
        }
    };
}
pub(crate) use js_regex;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ascii_semantics() {
        let otp = compile(r"\botp\b", true);
        assert!(otp.is_match("éOTPé")); // JS: é is a non-word char
        assert!(!otp.is_match("\u{212A}otp".replace('o', "\u{17F}").as_str()));
        let k = compile("k", true);
        assert!(!k.is_match("\u{212A}"));
        assert!(k.is_match("K"));
        let cls = compile("^[A-Z0-9]{3}$", true);
        assert!(cls.is_match("ab1"));
        assert!(!cls.is_match("\u{17F}ab"));
        let ws = compile(r"^\s$", false);
        assert!(ws.is_match("\u{FEFF}"));
        assert!(!ws.is_match("\u{85}"));
        let d = compile(r"^\d$", false);
        assert!(!d.is_match("٣"));
    }
}
