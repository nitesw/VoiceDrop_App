//! A deterministic "remove this word no matter what" filter, run
//! unconditionally on the *Raw Transcript* right after STT — before any
//! Cleanup Pass provider (including `None`) ever sees the text.
//!
//! This is intentionally NOT part of the Cleanup Pass: the whole reason it
//! exists is that Cleanup Pass quality/latency is bad enough on small local
//! models that users are steered toward `CleanupProviderKind::None`
//! (`docs/todos/0004-phase3-cleanup-pass.md`), and relying on an LLM to
//! reliably strip specific words would mean this either doesn't run at all
//! (`None`) or runs unreliably (a small model might miss a word, or delete
//! a similar-sounding one it shouldn't). A plain word-list filter has
//! neither failure mode.

use std::collections::HashSet;

/// Small built-in default — deliberately short and unsurprising (common
/// English profanity), not an attempt at a comprehensive filter. Users
/// extend this via their own words (see `Blocklist::new`); nothing here is
/// meant to be exhaustive on its own.
const DEFAULT_WORDS: &[&str] = &["fuck", "fucking", "shit", "bitch", "asshole", "bastard"];

pub struct Blocklist {
    words: HashSet<String>,
}

impl Blocklist {
    /// Merges the built-in default list with user-supplied words (matching
    /// is always case-insensitive, so callers don't need to normalize
    /// case themselves).
    pub fn new(custom_words: &[String]) -> Self {
        let mut words: HashSet<String> = DEFAULT_WORDS.iter().map(|s| s.to_lowercase()).collect();
        words.extend(custom_words.iter().map(|w| w.trim().to_lowercase()));
        words.remove("");
        Blocklist { words }
    }

    /// Removes every whole-word match (case-insensitive) of a blocklisted
    /// word from `text`, collapsing the whitespace/punctuation left behind
    /// so the result doesn't read as visibly gapped. Word-boundary aware:
    /// "assassin" is untouched by a "ass" entry.
    pub fn filter(&self, text: &str) -> String {
        if self.words.is_empty() {
            return text.to_string();
        }

        let tokens = tokenize(text);
        let kept: Vec<&str> = tokens
            .iter()
            .filter(|t| match t {
                Token::Word(w) => !self.words.contains(&w.to_lowercase()),
                Token::Other(_) => true,
            })
            .map(Token::as_str)
            .collect();

        normalize_whitespace(&kept.concat())
    }
}

enum Token<'a> {
    Word(&'a str),
    Other(&'a str),
}

impl<'a> Token<'a> {
    fn as_str(&self) -> &'a str {
        match self {
            Token::Word(s) | Token::Other(s) => s,
        }
    }
}

/// Splits `text` into maximal runs of "word" characters (alphanumeric or
/// apostrophe, so contractions like "don't" stay one token) and runs of
/// everything else (whitespace, punctuation).
fn tokenize(text: &str) -> Vec<Token<'_>> {
    let mut tokens = Vec::new();
    let mut start = 0;
    let mut in_word = None;

    for (i, c) in text.char_indices() {
        let is_word_char = c.is_alphanumeric() || c == '\'';
        match in_word {
            Some(true) if !is_word_char => {
                tokens.push(Token::Word(&text[start..i]));
                start = i;
                in_word = Some(false);
            }
            Some(false) if is_word_char => {
                tokens.push(Token::Other(&text[start..i]));
                start = i;
                in_word = Some(true);
            }
            None => in_word = Some(is_word_char),
            _ => {}
        }
    }
    if start < text.len() {
        match in_word {
            Some(true) => tokens.push(Token::Word(&text[start..])),
            _ => tokens.push(Token::Other(&text[start..])),
        }
    }
    tokens
}

/// Collapses runs of whitespace left behind by removed words into a single
/// space, trims stray spaces before punctuation (e.g. "that , right" from
/// removing a word right before a comma), and collapses the doubled-up
/// punctuation that leaves behind (e.g. removing the word between "well,"
/// and ", that" would otherwise leave "well,, that").
fn normalize_whitespace(text: &str) -> String {
    let collapsed: String = text.split_whitespace().collect::<Vec<_>>().join(" ");

    let mut result = String::with_capacity(collapsed.len());
    for c in collapsed.chars() {
        if matches!(c, ',' | '.' | '!' | '?' | ';' | ':') {
            while result.ends_with(' ') {
                result.pop();
            }
            if result.ends_with(c) {
                continue;
            }
        }
        result.push(c);
    }
    result.trim().to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn removes_default_word_case_insensitively() {
        let blocklist = Blocklist::new(&[]);
        assert_eq!(blocklist.filter("I said Fuck that"), "I said that");
    }

    #[test]
    fn does_not_touch_word_containing_blocklisted_substring() {
        let blocklist = Blocklist::new(&["ass".to_string()]);
        assert_eq!(blocklist.filter("call an assassin"), "call an assassin");
    }

    #[test]
    fn removes_custom_user_word() {
        let blocklist = Blocklist::new(&["voicedrop".to_string()]);
        assert_eq!(
            blocklist.filter("this is voicedrop testing"),
            "this is testing"
        );
    }

    #[test]
    fn removes_multiple_occurrences() {
        let blocklist = Blocklist::new(&[]);
        assert_eq!(blocklist.filter("shit this is shit"), "this is");
    }

    #[test]
    fn cleans_up_punctuation_spacing_after_removal() {
        let blocklist = Blocklist::new(&[]);
        assert_eq!(
            blocklist.filter("well, shit, that broke"),
            "well, that broke"
        );
    }

    #[test]
    fn text_with_no_blocklisted_words_is_unchanged_besides_whitespace_normalization() {
        let blocklist = Blocklist::new(&[]);
        assert_eq!(blocklist.filter("hello there friend"), "hello there friend");
    }

    #[test]
    fn empty_custom_words_are_ignored_not_matched_as_blank() {
        let blocklist = Blocklist::new(&["".to_string(), "  ".to_string()]);
        assert_eq!(blocklist.filter("hello there"), "hello there");
    }

    #[test]
    fn preserves_contractions_as_single_tokens() {
        let blocklist = Blocklist::new(&["dont".to_string()]);
        assert_eq!(blocklist.filter("I don't know"), "I don't know");
    }
}
