/// IntoArgs code taken from:
/// https://github.com/tmiasko/shell-words/blob/master/src/lib.rs
/// from this stack overflow question:
/// https://users.rust-lang.org/t/splitting-string-on-white-space-but-preserving-quoted-substring/73366/2
use core::mem;
use std::fmt::{Display, Formatter, Result as FmtResult};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ParseError;

impl Display for ParseError {
    fn fmt(&self, f: &mut Formatter) -> FmtResult {
        f.write_str("missing closing quote")
    }
}

impl std::error::Error for ParseError {}

enum State {
    /// Within a delimiter.
    Delimiter,
    /// After backslash, but before starting word.
    Backslash,
    /// Within an unquoted word.
    Unquoted,
    /// After backslash in an unquoted word.
    UnquotedBackslash,
    /// Within a single quoted word.
    SingleQuoted,
    /// Within a double quoted word.
    DoubleQuoted,
    /// After backslash inside a double quoted word.
    DoubleQuotedBackslash,
}

pub(crate) trait IntoArgs {
    fn try_into_args(&self) -> Result<Vec<String>, ParseError>;
}

impl<S: std::ops::Deref<Target = str>> IntoArgs for S {
    fn try_into_args(&self) -> Result<Vec<String>, ParseError> {
        use State::*;

        let mut words = Vec::new();
        let mut word = String::new();
        let mut chars = self.chars();
        let mut state = Delimiter;

        loop {
            let c = chars.next();
            state = match state {
                Delimiter => match c {
                    None => break,
                    Some('\'') => SingleQuoted,
                    Some('\"') => DoubleQuoted,
                    Some('\\') => Backslash,
                    Some('\t') | Some(' ') | Some('\n') => Delimiter,
                    Some(c) => {
                        word.push(c);
                        Unquoted
                    }
                },
                Backslash => match c {
                    None => {
                        word.push('\\');
                        words.push(mem::take(&mut word));
                        break;
                    }
                    Some('\n') => Delimiter,
                    Some(c) => {
                        word.push(c);
                        Unquoted
                    }
                },
                Unquoted => match c {
                    None => {
                        words.push(mem::take(&mut word));
                        break;
                    }
                    Some('\'') => SingleQuoted,
                    Some('\"') => DoubleQuoted,
                    Some('\\') => UnquotedBackslash,
                    Some('\t') | Some(' ') | Some('\n') => {
                        words.push(mem::take(&mut word));
                        Delimiter
                    }
                    Some(c) => {
                        word.push(c);
                        Unquoted
                    }
                },
                UnquotedBackslash => match c {
                    None => {
                        word.push('\\');
                        words.push(mem::take(&mut word));
                        break;
                    }
                    Some('\n') => Unquoted,
                    Some(c) => {
                        word.push(c);
                        Unquoted
                    }
                },
                SingleQuoted => match c {
                    None => return Err(ParseError),
                    Some('\'') => Unquoted,
                    Some(c) => {
                        word.push(c);
                        SingleQuoted
                    }
                },
                DoubleQuoted => match c {
                    None => return Err(ParseError),
                    Some('\"') => Unquoted,
                    Some('\\') => DoubleQuotedBackslash,
                    Some(c) => {
                        word.push(c);
                        DoubleQuoted
                    }
                },
                DoubleQuotedBackslash => match c {
                    None => return Err(ParseError),
                    Some('\n') => DoubleQuoted,
                    Some(c @ '$') | Some(c @ '`') | Some(c @ '"') | Some(c @ '\\') => {
                        word.push(c);
                        DoubleQuoted
                    }
                    Some(c) => {
                        word.push('\\');
                        word.push(c);
                        DoubleQuoted
                    }
                },
            }
        }

        Ok(words)
    }
}

pub fn count_quotes(input: &str, quote_stack: &mut Vec<char>) {
    let matches: Vec<char> = input
        .match_indices(&['"', '\''])
        .map(|(_, str)| str.chars().collect::<Vec<char>>()[0])
        .collect();

    for m in matches {
        match quote_stack.last() {
            Some(token) => {
                // if match is the same as last element in quote stack,
                // we've found the closing quote, so pop it off the stack
                if m == *token {
                    quote_stack.pop();
                }
                // if the match is not the same as the last element in then
                // quote stack, ignore it
            }
            None => {
                // if the quote stack is empty, push this match on it,
                // because it's an opening quote
                quote_stack.push(m);
            }
        }
    }
}
