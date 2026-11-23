use anyhow::{Context, Result};
use misaki_rs::{Fallback, G2P, Language, fallback::FallbackError};
use regex::Regex;
use std::{process::Command, time::Duration};

/// Use the installed eSpeak instead of the port's differently normalized backend.
struct EnglishFallback;

impl Fallback for EnglishFallback {
    fn phonemize(&self, word: &str) -> std::result::Result<String, FallbackError> {
        let result = preserve_punctuation(word, |part| {
            let bytes = crate::process::output(
                Command::new("espeak-ng")
                    .args(["-q", "--ipa", "--tie=^", "-v", "en-us", "--stdin"]),
                part.as_bytes(),
                Duration::from_secs(10),
                65536,
            )?;
            Ok(String::from_utf8_lossy(&bytes).trim().to_owned())
        });
        result
            .map(|ipa| espeak_to_misaki(&ipa))
            .map_err(|error| FallbackError::Espeak {
                word: word.into(),
                error: error.to_string(),
            })
    }
}

/// eSpeak discards punctuation. Preserve it around OOV fragments as the current
/// phonemizer frontend does, keeping decimal separators within numeric fragments.
fn preserve_punctuation(
    word: &str,
    mut phonemize: impl FnMut(&str) -> Result<String>,
) -> Result<String> {
    let chars: Vec<char> = word.chars().collect();
    let mut result = String::new();
    let mut fragment = String::new();
    for (index, c) in chars.iter().copied().enumerate() {
        let decimal = ".,".contains(c)
            && index > 0
            && chars[index - 1].is_ascii_digit()
            && chars.get(index + 1).is_some_and(char::is_ascii_digit);
        if ";:,.!?¡¿—…\"«»“”(){}[]".contains(c) && !decimal {
            if !fragment.trim().is_empty() {
                result.push_str(&phonemize(fragment.trim())?);
            }
            fragment.clear();
            result.push(c);
        } else {
            fragment.push(c);
        }
    }
    if !fragment.trim().is_empty() {
        result.push_str(&phonemize(fragment.trim())?);
    }
    Ok(result)
}

pub fn espeak_to_misaki(ipa: &str) -> String {
    let mut text = ipa.trim().to_owned();
    // The order and bare `e` mapping match misaki.espeak.EspeakFallback (American, version=None).
    for (old, new) in [
        ("ʔˌn\u{0329}", "ʔn"),
        ("ʔn\u{0329}", "ʔn"),
        ("a^ɪ", "I"),
        ("a^ʊ", "W"),
        ("d^ʒ", "ʤ"),
        ("e^ɪ", "A"),
        ("t^ʃ", "ʧ"),
        ("ɔ^ɪ", "Y"),
        ("ə^l", "ᵊl"),
        ("ʲo", "jo"),
        ("ʲə", "jə"),
        ("e", "A"),
        ("ʲ", ""),
        ("ɚ", "əɹ"),
        ("r", "ɹ"),
        ("x", "k"),
        ("ç", "k"),
        ("ɐ", "ə"),
        ("ɬ", "l"),
        ("\u{0303}", ""),
    ] {
        text = text.replace(old, new);
    }
    let syllabic = Regex::new(r"(\S)\u{0329}").expect("fixed syllabic expression");
    text = syllabic.replace_all(&text, "ᵊ$1").replace('\u{0329}', "");
    for (old, new) in [
        ("o^ʊ", "O"),
        ("ɜːɹ", "ɜɹ"),
        ("ɜː", "ɜɹ"),
        ("ɪə", "iə"),
        ("ː", ""),
        ("o", "ɔ"),
        ("ɾ", "T"),
        ("ʔ", "t"),
        ("^", ""),
    ] {
        text = text.replace(old, new);
    }
    text
}

pub struct English(G2P);

impl Default for English {
    fn default() -> Self {
        Self(G2P::with_fallback(
            Language::EnglishUS,
            Some(Box::new(EnglishFallback)),
        ))
    }
}

impl English {
    pub fn phonemes(&mut self, text: &str) -> Result<String> {
        self.0
            .g2p(text)
            .map(|(phonemes, _)| phonemes)
            .map_err(|error| anyhow::anyhow!(error))
            .context("English phonemization")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fallback_uses_the_python_symbols() {
        assert_eq!(
            espeak_to_misaki(" e a^ɪ o^ʊ ɜːɹ t^ʃ d^ʒ \n"),
            "A I O ɜɹ ʧ ʤ"
        );
        assert_eq!(espeak_to_misaki("n\u{0329} ɾ ʔ"), "ᵊn T t");
    }

    #[test]
    fn fallback_preserves_colons_but_keeps_decimal_numbers() {
        assert_eq!(
            preserve_punctuation("12:30", |s| Ok(format!("<{s}>"))).unwrap(),
            "<12>:<30>"
        );
        assert_eq!(
            preserve_punctuation("3.5!", |s| Ok(format!("<{s}>"))).unwrap(),
            "<3.5>!"
        );
    }
}
