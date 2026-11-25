use regex::Regex;
use std::sync::LazyLock;

static WORDS: LazyLock<Regex> = LazyLock::new(|| Regex::new("[a-z0-9']+").expect("fixed words"));
static NORMAL: LazyLock<Regex> =
    LazyLock::new(|| Regex::new("[a-z0-9]+").expect("fixed normalization"));
const WAKE: &[&str] = &[
    "tohru", "toru", "taro", "tarou", "taru", "tauru", "teru", "tooru", "tohro", "toro", "toroo",
    "torou", "torru", "torue", "toeru",
];
const FIRST: &[&str] = &["to", "too", "two", "toe", "tow", "toh"];
const SECOND: &[&str] = &["ru", "rue", "roo", "rew", "ro", "row", "who"];
const FILLERS: &[&str] = &[
    "hey", "hi", "hello", "hiya", "yo", "okay", "ok", "oh", "um", "uh", "umm", "uhm", "er", "erm",
    "ah", "hm", "hmm", "mm", "alright",
];
// Ordinary English words the recognizer also hears for the name; they wake only in the addressing position.
const LEADING_ONLY: &[&str] = &["taro", "toro", "rue", "row", "who"];
const BACK: &[&str] = &[
    "mm", "mmm", "mhm", "mhmm", "hm", "hmm", "hmmm", "m", "uh", "um", "umm", "ah", "oh", "eh",
    "er", "huh", "uhuh", "aha", "yeah", "yep", "yup", "ya", "ok", "okay", "right", "sure",
    "alright",
];
const TAIL: &[&str] = &[
    "a", "an", "and", "are", "as", "at", "be", "because", "but", "by", "er", "for", "from", "have",
    "hm", "hmm", "if", "in", "into", "is", "like", "my", "of", "on", "or", "our", "so", "that",
    "the", "their", "then", "this", "to", "uh", "um", "umm", "was", "were", "what", "when",
    "where", "which", "while", "who", "will", "with", "would", "your",
];

pub fn words(text: &str) -> Vec<String> {
    WORDS
        .find_iter(&text.to_lowercase())
        .map(|m| m.as_str().into())
        .collect()
}

pub fn normalize(text: &str) -> String {
    NORMAL
        .find_iter(&text.to_lowercase())
        .map(|m| m.as_str())
        .collect::<Vec<_>>()
        .join(" ")
}

pub fn sentence(text: &str) -> String {
    let text = text
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .to_lowercase();
    static I: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"\bi\b").expect("fixed pronoun"));
    let text = I.replace_all(&text, "I").into_owned();
    let mut chars = text.chars();
    chars
        .next()
        .map(|c| c.to_uppercase().collect::<String>() + chars.as_str())
        .unwrap_or_default()
}

/// The index and length of the first name form addressing Tohru.
pub fn wake_match(tokens: &[String]) -> Option<(usize, usize)> {
    let is = |set: &[&str], t: &String| set.contains(&t.as_str());
    let lead = tokens.iter().take_while(|t| is(FILLERS, t)).count();
    tokens.iter().enumerate().find_map(|(i, token)| {
        let form = if is(WAKE, token) {
            &tokens[i..=i]
        } else if is(FIRST, token) && tokens.get(i + 1).is_some_and(|t| is(SECOND, t)) {
            &tokens[i..i + 2]
        } else {
            return None;
        };
        (i == lead || !form.iter().any(|t| is(LEADING_ONLY, t))).then_some((i, form.len()))
    })
}

/// How many leading tokens a rejected candidate's diagnostic may show.
pub fn wake_split_width(tokens: &[String]) -> usize {
    if tokens.first().is_some_and(|t| FIRST.contains(&t.as_str())) {
        2
    } else {
        1
    }
}

/// The request with the name and leading fillers removed, empty for the name alone.
pub fn wake_split(text: &str) -> Option<String> {
    let tokens = words(text);
    let (at, length) = wake_match(&tokens)?;
    let before = tokens[..at]
        .iter()
        .skip_while(|t| FILLERS.contains(&t.as_str()));
    let request: Vec<&str> = before
        .chain(&tokens[at + length..])
        .map(String::as_str)
        .collect();
    Some(sentence(&request.join(" ")))
}

pub fn backchannel(text: &str) -> bool {
    words(text).iter().all(|w| BACK.contains(&w.as_str()))
}

pub fn unfinished(text: &str) -> bool {
    text.trim_end().ends_with([',', ';', ':', '—', '-'])
        || normalize(text)
            .split_whitespace()
            .last()
            .is_some_and(|w| TAIL.contains(&w))
}

pub fn explicit_hold(text: &str) -> bool {
    let text = normalize(text);
    [
        "hold on",
        "let me think",
        "let me work this out",
        "give me a moment",
        "wait a moment",
    ]
    .iter()
    .any(|end| text == *end || text.ends_with(&format!(" {end}")))
}

pub fn explicit_handoff(text: &str) -> bool {
    let text = normalize(text);
    ["your turn", "over to you", "go ahead", "what do you think"]
        .iter()
        .any(|end| text == *end || text.ends_with(&format!(" {end}")))
}

pub fn join(first: &str, second: &str) -> String {
    let first = first.trim().trim_end_matches(['.', '…']);
    let second = second.trim();
    if first.is_empty() {
        return second.into();
    }
    if second.is_empty() {
        return first.into();
    }
    let mut chars = second.chars();
    let c = chars.next().unwrap_or_default();
    let second = if second.chars().count() > 1
        && c.is_uppercase()
        && !second.starts_with("I ")
        && !second.starts_with("I'")
    {
        c.to_lowercase().collect::<String>() + chars.as_str()
    } else {
        second.into()
    };
    format!("{first} {second}")
}

pub fn speakable(text: &str) -> String {
    static FENCED: LazyLock<Regex> =
        LazyLock::new(|| Regex::new(r"(?s)```.*?```|~~~.*?~~~").expect("fixed fences"));
    static OPEN: LazyLock<Regex> =
        LazyLock::new(|| Regex::new(r"(?s)(```|~~~).*").expect("fixed open fence"));
    static INLINE: LazyLock<Regex> =
        LazyLock::new(|| Regex::new(r"`([^`]*)`").expect("fixed inline"));
    static LINKS: LazyLock<Regex> =
        LazyLock::new(|| Regex::new(r"!?\[([^\]]*)\]\([^)]*\)").expect("fixed links"));
    static URL: LazyLock<Regex> =
        LazyLock::new(|| Regex::new(r"https?://\S+").expect("fixed urls"));
    static BULLET: LazyLock<Regex> =
        LazyLock::new(|| Regex::new(r"(?m)^\s*[-•]\s+").expect("fixed bullets"));
    let text = FENCED.replace_all(text, " (code in the popup) ");
    let text = OPEN.replace_all(&text, " (code in the popup) ");
    let text = INLINE.replace_all(&text, "$1");
    let text = LINKS.replace_all(&text, "$1");
    let text = URL.replace_all(&text, " (link in the chat) ");
    let text: String = text.chars().filter(|c| !"*_#>".contains(*c)).collect();
    BULLET
        .replace_all(&text, "")
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

pub const SPOKEN_CHARS: usize = 16384;

#[derive(Default)]
pub struct SpeechBuffer {
    pending: String,
    total: usize,
    closed: bool,
    emitted: bool,
}

impl SpeechBuffer {
    pub fn feed(&mut self, text: &str, final_part: bool) {
        if self.closed {
            return;
        }
        if text.chars().count() > SPOKEN_CHARS - self.total {
            self.pending.push_str(" The rest is in the chat.");
            self.closed = true;
        } else {
            self.pending.push_str(text);
            self.total += text.chars().count();
            self.closed = final_part;
        }
    }

    pub fn finished(&self) -> bool {
        self.closed && self.pending.is_empty()
    }

    pub fn next_part(&mut self) -> Option<String> {
        if self.pending.is_empty() {
            return None;
        }
        static END: LazyLock<Regex> =
            LazyLock::new(|| Regex::new(r#"[.!?…]["')]*(?:\s+|$)"#).expect("fixed sentence end"));
        let tail = self.closed
            && self.emitted
            && self.pending.chars().count() <= 180
            && !self.pending.trim_end().ends_with(['.', '!', '?', '…'])
            && END.find_iter(&self.pending).count() <= 1;
        let cut = if tail {
            Some(self.pending.len())
        } else {
            END.find(&self.pending)
                .and_then(|m| {
                    let prefix = &self.pending[..m.end()];
                    let boundary = self.closed
                        || m.end() < self.pending.len()
                        || self.pending.ends_with(char::is_whitespace);
                    let balanced = prefix.matches("```").count().is_multiple_of(2)
                        && prefix.matches("~~~").count().is_multiple_of(2)
                        && prefix.matches('`').count().is_multiple_of(2)
                        && prefix.matches('[').count() == prefix.matches(']').count()
                        && prefix.matches('(').count() == prefix.matches(')').count();
                    (boundary && balanced).then_some(m.end())
                })
                .or_else(|| self.closed.then_some(self.pending.len()))
        };
        let cut = cut?;
        let part = self.pending[..cut].to_owned();
        self.pending.drain(..cut);
        let part = speakable(&part);
        if part.is_empty() {
            return self.next_part();
        }
        self.emitted = true;
        Some(part)
    }
}

/// Python's sentence-length bound also limits each inference operation.
pub fn chunks(text: &str) -> Vec<String> {
    static SPLIT: LazyLock<Regex> =
        LazyLock::new(|| Regex::new(r"[.!?…]\s+").expect("fixed sentence split"));
    let mut pieces = Vec::new();
    let mut start = 0;
    for m in SPLIT.find_iter(text.trim()) {
        pieces.push(
            text.trim()[start
                ..m.start()
                    + text.trim()[m.start()..]
                        .chars()
                        .next()
                        .map_or(0, char::len_utf8)]
                .trim()
                .to_owned(),
        );
        start = m.end();
    }
    pieces.push(text.trim()[start..].into());
    let mut parts = Vec::new();
    let mut current = String::new();
    for piece in pieces {
        let mut piece = piece;
        while piece.chars().count() > 220 {
            let bound = piece
                .char_indices()
                .nth(220)
                .map_or(piece.len(), |(i, _)| i);
            let cut = piece[..bound]
                .rfind(' ')
                .filter(|i| *i > 0)
                .unwrap_or(bound);
            parts.push(piece[..cut].into());
            piece = piece[cut..].trim_start().into();
        }
        if !current.is_empty()
            && (parts.is_empty() || current.chars().count() + piece.chars().count() + 1 > 220)
        {
            parts.push(std::mem::take(&mut current));
            current = piece;
        } else {
            if !current.is_empty() {
                current.push(' ');
            }
            current.push_str(&piece);
        }
    }
    if !current.trim().is_empty() {
        parts.push(current);
    }
    parts.into_iter().filter(|p| !p.trim().is_empty()).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_name_wakes_anywhere_and_leaves_the_request() {
        for (transcript, request) in [
            ("Tohru what time is it", Some("What time is it")),
            ("toru", Some("")),
            ("TO RU tell me a joke", Some("Tell me a joke")),
            ("hey Tohru, what's the weather", Some("What's the weather")),
            (
                "okay tohru what's on my calendar",
                Some("What's on my calendar"),
            ),
            ("hey tohru", Some("")),
            ("so Tohru, what's next", Some("So what's next")),
            (
                "can you check this Tohru what's failing",
                Some("Can you check this what's failing"),
            ),
            ("can you check this Tohru", Some("Can you check this")),
            ("hey can you check this tohru", Some("Can you check this")),
            ("can you check this to ru", Some("Can you check this")),
            ("Tohru tell me about Tohru", Some("Tell me about tohru")),
            ("taro what's up", Some("What's up")),
            ("hey toro", Some("")),
            ("too roo what is this", Some("What is this")),
            ("what time is it", None),
            ("hello there", None),
            ("the tour tore through a torus of tarot cards totally", None),
            ("I had taro for lunch", None),
            ("let's order some toro", None),
            ("who did you give it to who", None),
            ("I want to row", None),
            ("that is Tohru's", None),
        ] {
            assert_eq!(wake_split(transcript).as_deref(), request, "{transcript}");
        }
    }

    #[test]
    fn wake_and_continuation() {
        assert_eq!(
            wake_split("too roo what is this"),
            Some("What is this".into())
        );
        assert_eq!(join("I think…", "Maybe a map"), "I think maybe a map");
    }

    #[test]
    fn streaming_holds_code_and_flushes_once() {
        let mut b = SpeechBuffer::default();
        b.feed("Here is `a. ", false);
        assert!(b.next_part().is_none());
        b.feed("b`. Next", false);
        assert!(b.next_part().is_none());
        b.feed(" words", true);
        assert_eq!(b.next_part(), Some("Here is a. b. Next words".into()));
        assert!(b.next_part().is_none());
        assert!(b.finished());
    }

    #[test]
    fn overflow_is_bounded() {
        let mut b = SpeechBuffer::default();
        b.feed(&"a".repeat(SPOKEN_CHARS + 1), false);
        assert_eq!(b.next_part(), Some("The rest is in the chat.".into()));
        assert!(b.finished());
    }
}
