/// Quotes one word for a POSIX shell, leaving plainly safe words unchanged (`shlex.quote`).
#[must_use]
pub fn quote(word: &str) -> String {
    if word.is_empty() {
        return "''".to_owned();
    }
    let safe = word
        .bytes()
        .all(|byte| byte.is_ascii_alphanumeric() || b"_@%+=:,./-".contains(&byte));
    if safe {
        word.to_owned()
    } else {
        format!("'{}'", word.replace('\'', "'\"'\"'"))
    }
}

/// Joins words into one shell command line.
#[must_use]
pub fn join<S: AsRef<str>>(words: &[S]) -> String {
    words
        .iter()
        .map(|word| quote(word.as_ref()))
        .collect::<Vec<_>>()
        .join(" ")
}

#[derive(Debug, PartialEq, Eq, thiserror::Error)]
pub enum SplitError {
    #[error("No closing quotation")]
    Unterminated,
    #[error("No escaped character")]
    Escape,
}

/// One lexical shell token: a word, or a run of control characters.
#[derive(Debug, PartialEq, Eq)]
pub enum Token {
    Word(String),
    Control(String),
}

const CONTROL: &str = "();<>|&`\n";

/// Splits a command line the way a POSIX shell's lexer separates words and operators.
///
/// Quotes and backslashes are honoured, `#` starts a comment only at the start of a word,
/// and a newline or backquote is a separator like `;`.
pub fn tokens(text: &str) -> Result<Vec<Token>, SplitError> {
    let mut result = Vec::new();
    let mut chars = text.chars().peekable();
    let mut word = String::new();
    let mut in_word = false;
    let finish = |word: &mut String, in_word: &mut bool, result: &mut Vec<Token>| {
        if *in_word {
            result.push(Token::Word(std::mem::take(word)));
            *in_word = false;
        }
    };
    while let Some(character) = chars.next() {
        match character {
            ' ' | '\t' | '\r' => finish(&mut word, &mut in_word, &mut result),
            '#' if !in_word => {
                while chars.peek().is_some_and(|next| *next != '\n') {
                    chars.next();
                }
            }
            c if CONTROL.contains(c) => {
                finish(&mut word, &mut in_word, &mut result);
                let mut run = String::from(c);
                if c != '\n' && c != '`' {
                    while let Some(next) = chars.next_if(|next| "();<>|&".contains(*next)) {
                        run.push(next);
                    }
                }
                result.push(Token::Control(run));
            }
            '\'' => {
                in_word = true;
                loop {
                    match chars.next() {
                        Some('\'') => break,
                        Some(inner) => word.push(inner),
                        None => return Err(SplitError::Unterminated),
                    }
                }
            }
            '"' => {
                in_word = true;
                loop {
                    match chars.next() {
                        Some('"') => break,
                        Some('\\') => match chars.next() {
                            Some(escaped @ ('"' | '\\' | '$' | '`')) => word.push(escaped),
                            Some('\n') => {}
                            Some(other) => {
                                word.push('\\');
                                word.push(other);
                            }
                            None => return Err(SplitError::Unterminated),
                        },
                        Some(inner) => word.push(inner),
                        None => return Err(SplitError::Unterminated),
                    }
                }
            }
            '\\' => match chars.next() {
                Some('\n') => {}
                Some(escaped) => {
                    in_word = true;
                    word.push(escaped);
                }
                None => return Err(SplitError::Escape),
            },
            other => {
                in_word = true;
                word.push(other);
            }
        }
    }
    finish(&mut word, &mut in_word, &mut result);
    Ok(result)
}

/// Splits like `shlex.split`: words only, no operators recognised.
pub fn split(text: &str) -> Result<Vec<String>, SplitError> {
    let mut words = Vec::new();
    let mut chars = text.chars();
    let mut word = String::new();
    let mut in_word = false;
    while let Some(character) = chars.next() {
        match character {
            ' ' | '\t' | '\r' | '\n' => {
                if in_word {
                    words.push(std::mem::take(&mut word));
                    in_word = false;
                }
            }
            '\'' => {
                in_word = true;
                loop {
                    match chars.next() {
                        Some('\'') => break,
                        Some(inner) => word.push(inner),
                        None => return Err(SplitError::Unterminated),
                    }
                }
            }
            '"' => {
                in_word = true;
                loop {
                    match chars.next() {
                        Some('"') => break,
                        Some('\\') => match chars.next() {
                            Some(escaped @ ('"' | '\\')) => word.push(escaped),
                            Some(other) => {
                                word.push('\\');
                                word.push(other);
                            }
                            None => return Err(SplitError::Unterminated),
                        },
                        Some(inner) => word.push(inner),
                        None => return Err(SplitError::Unterminated),
                    }
                }
            }
            '\\' => {
                in_word = true;
                word.push(chars.next().ok_or(SplitError::Escape)?);
            }
            other => {
                in_word = true;
                word.push(other);
            }
        }
    }
    if in_word {
        words.push(word);
    }
    Ok(words)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn quoting_round_trips_through_split() {
        for word in ["plain", "", "it's", "a b", "$HOME", "x\"y"] {
            assert_eq!(split(&quote(word)).unwrap(), vec![word.to_owned()]);
        }
        assert_eq!(quote("/usr/bin/tohru-idle"), "/usr/bin/tohru-idle");
        assert_eq!(join(&["a b", "c"]), "'a b' c");
    }

    #[test]
    fn operators_and_comments_are_separated() {
        let words = |text| {
            tokens(text)
                .unwrap()
                .into_iter()
                .map(|token| match token {
                    Token::Word(word) | Token::Control(word) => word,
                })
                .collect::<Vec<_>>()
        };
        assert_eq!(words("a&&b|c"), ["a", "&&", "b", "|", "c"]);
        assert_eq!(
            words("echo a#b # gone\nsleep 1"),
            ["echo", "a#b", "\n", "sleep", "1"]
        );
        assert_eq!(words("printf 'cargo test'"), ["printf", "cargo test"]);
        assert_eq!(tokens("'open"), Err(SplitError::Unterminated));
    }
}
