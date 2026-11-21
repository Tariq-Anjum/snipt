//! JS `String.prototype.trim()` for passphrases: strips U+FEFF and the full
//! ECMAScript whitespace set (Rust `str::trim` does not strip U+FEFF and does
//! strip U+0085). Shared implementation lives in `sms_types::js`.

pub use sms_types::js::{is_js_whitespace, trim as js_trim};

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strips_bom_like_js() {
        assert_eq!(
            js_trim("\u{FEFF}my-secret-pass-2026\u{FEFF}\n"),
            "my-secret-pass-2026"
        );
        assert_ne!("\u{FEFF}x".trim(), "x");
        assert_eq!(js_trim("\u{85}x"), "\u{85}x");
    }
}
