//! Port of translate.ts — offline script/language detection + phrasebook
//! translation for hi/ur/hinglish/mr/bn/ta/te → en.

use std::collections::HashSet;
use std::sync::OnceLock;

use regex::Regex;
use serde::{Deserialize, Serialize};
use sms_types::js;

use crate::jsre::{compile, js_regex};
use crate::translate_tables::*;

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[serde(rename_all = "lowercase")]
pub enum DetectedLanguageCode {
    En,
    Hi,
    Ur,
    Hinglish,
    Mr,
    Bn,
    Ta,
    Te,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct LanguageInfo {
    pub code: DetectedLanguageCode,
    pub label: String,
    pub native_label: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub is_rtl: Option<bool>,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct TranslationResult {
    pub language: LanguageInfo,
    pub needs_translation: bool,
    pub translated_text: String,
    pub original_text: String,
    pub engine_label: String,
}

pub fn language_info(code: DetectedLanguageCode) -> LanguageInfo {
    use DetectedLanguageCode::*;
    let (label, native, rtl) = match code {
        En => ("English", "English", None),
        Hi => ("Hindi", "हिन्दी", None),
        Ur => ("Urdu", "اردو", Some(true)),
        Hinglish => ("Hinglish", "Hinglish", None),
        Mr => ("Marathi", "मराठी", None),
        Bn => ("Bengali", "বাংলা", None),
        Ta => ("Tamil", "தமிழ்", None),
        Te => ("Telugu", "తెలుగు", None),
    };
    LanguageInfo {
        code,
        label: label.into(),
        native_label: native.into(),
        is_rtl: rtl,
    }
}

fn count_in(s: &str, lo: char, hi: char) -> usize {
    s.chars().filter(|c| (lo..=hi).contains(c)).count()
}

fn sets() -> &'static (HashSet<&'static str>, HashSet<&'static str>) {
    static S: OnceLock<(HashSet<&'static str>, HashSet<&'static str>)> = OnceLock::new();
    S.get_or_init(|| {
        (
            HINGLISH_MARKERS.iter().copied().collect(),
            HINGLISH_STRONG.iter().copied().collect(),
        )
    })
}

pub fn detect_message_language(text: &str) -> LanguageInfo {
    use DetectedLanguageCode::*;
    let trimmed = js::trim(text);
    if trimmed.is_empty() {
        return language_info(En);
    }
    if count_in(trimmed, '\u{0600}', '\u{06FF}') >= 2 {
        return language_info(Ur);
    }
    if count_in(trimmed, '\u{0B80}', '\u{0BFF}') >= 2 {
        return language_info(Ta);
    }
    if count_in(trimmed, '\u{0C00}', '\u{0C7F}') >= 2 {
        return language_info(Te);
    }
    if count_in(trimmed, '\u{0980}', '\u{09FF}') >= 2 {
        return language_info(Bn);
    }
    if count_in(trimmed, '\u{0900}', '\u{097F}') >= 2 {
        let mut marathi_hits = 0;
        if trimmed.contains('ळ') {
            marathi_hits += 2;
        }
        for marker in MARATHI_MARKERS {
            if *marker != "ळ" && trimmed.contains(marker) {
                marathi_hits += 1;
            }
        }
        if marathi_hits >= 2 && !trimmed.contains("है") && !trimmed.contains("आपका") {
            return language_info(Mr);
        }
        return language_info(Hi);
    }

    let lowered: String = trimmed
        .to_lowercase()
        .chars()
        .map(|c| {
            if c.is_ascii_lowercase() || js::is_js_whitespace(c) {
                c
            } else {
                ' '
            }
        })
        .collect();
    let (markers, strong) = sets();
    let (mut hits, mut strong_hits) = (0, 0);
    for w in lowered
        .split(js::is_js_whitespace)
        .filter(|w| !w.is_empty())
    {
        if markers.contains(w) {
            hits += 1;
        }
        if strong.contains(w) {
            strong_hits += 1;
        }
    }
    if strong_hits >= 2 || (strong_hits >= 1 && hits >= 3) {
        return language_info(Hinglish);
    }
    language_info(En)
}

type Compiled = Vec<(Regex, &'static str)>;

fn compiled(code: DetectedLanguageCode) -> &'static Compiled {
    use DetectedLanguageCode::*;
    static TABLES: OnceLock<Vec<(DetectedLanguageCode, Compiled)>> = OnceLock::new();
    let tables = TABLES.get_or_init(|| {
        [
            (Hi, HINDI_RULES),
            (Ur, URDU_RULES),
            (Hinglish, HINGLISH_RULES),
            (Mr, MARATHI_RULES),
            (Ta, TAMIL_RULES),
            (Te, TELUGU_RULES),
            (Bn, BENGALI_RULES),
        ]
        .into_iter()
        .map(|(c, rules)| {
            let v = rules
                .iter()
                .map(|(p, ci, rep)| (compile(p, *ci), *rep))
                .collect();
            (c, v)
        })
        .collect()
    });
    &tables
        .iter()
        .find(|(c, _)| *c == code)
        .expect("rule table")
        .1
}

js_regex!(ws_run, r"\s+");
js_regex!(ws_before_punct, r"\s+([.,!?])");
js_regex!(rs_space, r"Rs\.\s+");

fn normalize_translated_sentence(text: &str) -> String {
    let a = ws_run().replace_all(text, " ");
    let b = ws_before_punct().replace_all(&a, "${1}");
    let c = rs_space().replace_all(&b, "Rs.");
    let cleaned = js::trim(&c);
    let mut chars = cleaned.chars();
    match chars.next() {
        None => String::new(),
        // JS `charAt(0)` takes one UTF-16 unit: an astral first char is a lone
        // high surrogate whose toUpperCase() is itself.
        Some(first) if first.len_utf16() == 2 => cleaned.to_string(),
        Some(first) => {
            let mut s: String = first.to_uppercase().collect();
            s.push_str(chars.as_str());
            s
        }
    }
}

pub fn translate_sms_to_english(text: &str) -> TranslationResult {
    let language = detect_message_language(text);
    if language.code == DetectedLanguageCode::En {
        return TranslationResult {
            language,
            needs_translation: false,
            translated_text: text.to_string(),
            original_text: text.to_string(),
            engine_label: "Native English".into(),
        };
    }
    let mut output = text.to_string();
    for (re, rep) in compiled(language.code) {
        if let std::borrow::Cow::Owned(s) = re.replace_all(&output, *rep) {
            output = s;
        }
    }
    TranslationResult {
        language,
        needs_translation: true,
        translated_text: normalize_translated_sentence(&output),
        original_text: text.to_string(),
        engine_label: "Offline Lightweight Pack (0 KB/req)".into(),
    }
}
