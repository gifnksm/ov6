use alloc::{borrow::Cow, vec::Vec};
use core::{fmt, slice};

use ov6_user_lib::os_str::{OsStr, OsString};

const SYMBOLS: &[u8] = b"<|>&;()";

#[derive(Debug, Clone, PartialEq, Eq, derive_more::From)]
pub enum Token<'s> {
    Str(Cow<'s, OsStr>),
    Punct(Punct),
}

impl fmt::Display for Token<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Str(s) => fmt::Display::fmt(&OsStr::new(s).display(), f),
            Self::Punct(p) => fmt::Display::fmt(p, f),
        }
    }
}

impl PartialEq<str> for Token<'_> {
    fn eq(&self, other: &str) -> bool {
        match self {
            Token::Str(s) => OsStr::new(s) == other,
            Token::Punct(_) => false,
        }
    }
}

impl PartialEq<Punct> for Token<'_> {
    fn eq(&self, other: &Punct) -> bool {
        match self {
            Token::Punct(p) => p == other,
            Token::Str(_) => false,
        }
    }
}

impl Token<'_> {
    pub fn as_punct(&self) -> Option<Punct> {
        let Self::Punct(p) = self else {
            return None;
        };
        Some(*p)
    }

    pub fn into_owned(self) -> Token<'static> {
        match self {
            Self::Str(s) => Token::Str(s.into_owned().into()),
            Self::Punct(p) => Token::Punct(p),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Punct {
    Pipe,
    AndAnd,
    OrOr,
    LParen,
    RParen,
    Semicolon,
    And,
    Lt,
    Gt,
    GtGt,
}

impl Punct {
    fn as_str(self) -> &'static str {
        match self {
            Self::Pipe => "|",
            Self::LParen => "(",
            Self::RParen => ")",
            Self::Semicolon => ";",
            Self::And => "&",
            Self::AndAnd => "&&",
            Self::OrOr => "||",
            Self::Lt => "<",
            Self::Gt => ">",
            Self::GtGt => ">>",
        }
    }
}

impl fmt::Display for Punct {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Display::fmt(self.as_str(), f)
    }
}

#[derive(Debug, Clone)]
struct PeekableChars<'a> {
    chars: slice::Iter<'a, u8>,
}

impl Iterator for PeekableChars<'_> {
    type Item = u8;

    fn next(&mut self) -> Option<u8> {
        self.chars.next().copied()
    }
}

impl<'a> PeekableChars<'a> {
    fn new<S>(input: &'a S) -> Self
    where
        S: AsRef<[u8]> + ?Sized,
    {
        Self {
            chars: input.as_ref().iter(),
        }
    }

    fn next_if<F>(&mut self, f: F) -> Option<u8>
    where
        F: FnOnce(u8) -> bool,
    {
        let mut chars = self.chars.clone();
        let c = chars.next().copied()?;
        if !f(c) {
            return None;
        }
        self.chars = chars;
        Some(c)
    }

    fn next_if_eq(&mut self, c: u8) -> Option<u8> {
        self.next_if(|x| c == x)
    }

    fn as_slice(&self) -> &'a [u8] {
        self.chars.as_slice()
    }
}

#[derive(Debug, thiserror::Error)]
pub enum TokenizeError {
    #[error("incomplete escape sequence")]
    IncompleteEscape,
    #[error("unterminated double quote")]
    UnterminatedDoubleQuote,
    #[error("unterminated single quote")]
    UnterminatedSingleQuote,
}

pub struct Tokenizer<'a> {
    chars: PeekableChars<'a>,
}

impl<'a> Tokenizer<'a> {
    pub fn new<S>(input: &'a S) -> Self
    where
        S: AsRef<[u8]> + ?Sized,
    {
        Self {
            chars: PeekableChars::new(input),
        }
    }

    fn next_str(&mut self) -> Result<Option<Cow<'a, OsStr>>, TokenizeError> {
        let start = self.chars.as_slice();
        if start.is_empty() {
            return Ok(None);
        }

        let mut in_double_quotes = false;
        let mut in_single_quotes = false;
        let mut escaped = false;
        let mut needs_allocation = false;

        // Counting phase
        while !self.chars.as_slice().is_empty() {
            if escaped {
                escaped = false;
                needs_allocation = true;
                self.chars.next();
                continue;
            }
            if self
                .chars
                .next_if(|c| !in_single_quotes && c == b'\\')
                .is_some()
            {
                escaped = true;
                needs_allocation = true;
                continue;
            }
            if self
                .chars
                .next_if(|c| !in_single_quotes && c == b'\"')
                .is_some()
            {
                in_double_quotes = !in_double_quotes;
                needs_allocation = true;
                continue;
            }
            if self
                .chars
                .next_if(|c| !in_double_quotes && c == b'\'')
                .is_some()
            {
                in_single_quotes = !in_single_quotes;
                needs_allocation = true;
                continue;
            }
            if self
                .chars
                .next_if(|c| {
                    in_double_quotes
                        || in_single_quotes
                        || (!c.is_ascii_whitespace() && !SYMBOLS.contains(&c))
                })
                .is_none()
            {
                break;
            }
        }

        if escaped {
            return Err(TokenizeError::IncompleteEscape);
        }
        if in_double_quotes {
            return Err(TokenizeError::UnterminatedDoubleQuote);
        }
        if in_single_quotes {
            return Err(TokenizeError::UnterminatedSingleQuote);
        }

        let input_len = start.len() - self.chars.as_slice().len();
        let input = &start[..input_len];

        if !needs_allocation {
            return Ok(Some(Cow::Borrowed(OsStr::from_bytes(input))));
        }

        let mut result = Vec::with_capacity(input.len());
        let mut escaped = false;

        // Constructing the actual string
        for ch in input.iter().copied() {
            if escaped {
                result.push(ch);
                escaped = false;
                continue;
            }
            if ch == b'\\' && !in_single_quotes {
                escaped = true;
                continue;
            }
            if ch == b'\"' && !in_single_quotes {
                in_double_quotes = !in_double_quotes;
                continue;
            }
            if ch == b'\'' && !in_double_quotes {
                in_single_quotes = !in_single_quotes;
                continue;
            }
            result.push(ch);
        }

        Ok(Some(Cow::Owned(OsString::from_vec(result))))
    }

    fn next_token(&mut self) -> Result<Option<Token<'a>>, TokenizeError> {
        while self.chars.next_if(|c| c.is_ascii_whitespace()).is_some() {}
        let token: Token<'_> = if self.chars.next_if_eq(b'|').is_some() {
            if self.chars.next_if_eq(b'|').is_some() {
                Punct::OrOr.into()
            } else {
                Punct::Pipe.into()
            }
        } else if self.chars.next_if_eq(b'(').is_some() {
            Punct::LParen.into()
        } else if self.chars.next_if_eq(b')').is_some() {
            Punct::RParen.into()
        } else if self.chars.next_if_eq(b';').is_some() {
            Punct::Semicolon.into()
        } else if self.chars.next_if_eq(b'&').is_some() {
            if self.chars.next_if_eq(b'&').is_some() {
                Punct::AndAnd.into()
            } else {
                Punct::And.into()
            }
        } else if self.chars.next_if_eq(b'<').is_some() {
            Punct::Lt.into()
        } else if self.chars.next_if_eq(b'>').is_some() {
            if self.chars.next_if_eq(b'>').is_some() {
                Punct::GtGt.into()
            } else {
                Punct::Gt.into()
            }
        } else {
            return Ok(self.next_str()?.map(Into::into));
        };
        Ok(Some(token))
    }
}

impl<'a> Iterator for Tokenizer<'a> {
    type Item = Result<Token<'a>, TokenizeError>;

    fn next(&mut self) -> Option<Self::Item> {
        self.next_token().transpose()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[track_caller]
    fn assert_next_is_str(s: &mut Tokenizer, expected: &str) {
        let token = s.next().unwrap().unwrap();
        assert_eq!(token, Token::Str(OsStr::new(expected).into()));
    }

    #[track_caller]
    fn assert_next_is_punct(s: &mut Tokenizer, expected: Punct) {
        let token = s.next().unwrap().unwrap();
        assert_eq!(token, Token::Punct(expected));
    }

    #[test]
    fn test_str() {
        let mut s = Tokenizer::new("hello world");
        assert_next_is_str(&mut s, "hello");
        assert_next_is_str(&mut s, "world");
        assert!(s.next().is_none());

        let mut s = Tokenizer::new(r#""hello world""#);
        assert_next_is_str(&mut s, "hello world");
        assert!(s.next().is_none());

        let mut s = Tokenizer::new("'hello world'");
        assert_next_is_str(&mut s, "hello world");
        assert!(s.next().is_none());

        let mut s = Tokenizer::new(r"hello\ world");
        assert_next_is_str(&mut s, "hello world");
        assert!(s.next().is_none());
    }

    #[test]
    fn test_str_complex_cases() {
        let mut s = Tokenizer::new(r#"hello "world" 'test' \n \t \\"#);
        assert_next_is_str(&mut s, "hello");
        assert_next_is_str(&mut s, "world");
        assert_next_is_str(&mut s, "test");
        assert_next_is_str(&mut s, "n");
        assert_next_is_str(&mut s, "t");
        assert_next_is_str(&mut s, "\\");
        assert!(s.next().is_none());

        let mut s = Tokenizer::new(r#""hello \"world\"""#);
        assert_next_is_str(&mut s, "hello \"world\"");
        assert!(s.next().is_none());

        let mut s = Tokenizer::new(r"'hello \'world\'''");
        assert_next_is_str(&mut s, "hello \\world\'");
        assert!(s.next().is_none());

        let mut s = Tokenizer::new(r"hello\ world\!");
        assert_next_is_str(&mut s, "hello world!");
        assert!(s.next().is_none());

        let mut s = Tokenizer::new(r#""hello\nworld""#);
        assert_next_is_str(&mut s, "hellonworld");
        assert!(s.next().is_none());

        let mut s = Tokenizer::new(r"'hello\nworld'");
        assert_next_is_str(&mut s, r"hello\nworld");
        assert!(s.next().is_none());
    }

    #[test]
    fn test_str_incomplete_escape() {
        let mut s = Tokenizer::new("hello\\");
        assert!(matches!(
            s.next(),
            Some(Err(TokenizeError::IncompleteEscape))
        ));
        assert!(s.next().is_none());
    }

    #[test]
    fn test_str_unterminated_double_quotes() {
        let mut s = Tokenizer::new("hello\"world");
        assert!(matches!(
            s.next(),
            Some(Err(TokenizeError::UnterminatedDoubleQuote))
        ));
        assert!(s.next().is_none());
    }

    #[test]
    fn test_str_unterminated_single_quotes() {
        let mut s = Tokenizer::new("hello\'world");
        assert!(matches!(
            s.next(),
            Some(Err(TokenizeError::UnterminatedSingleQuote))
        ));
        assert!(s.next().is_none());
    }

    #[test]
    fn test_punctuations() {
        let mut s = Tokenizer::new("|&;()");
        assert_next_is_punct(&mut s, Punct::Pipe);
        assert_next_is_punct(&mut s, Punct::And);
        assert_next_is_punct(&mut s, Punct::Semicolon);
        assert_next_is_punct(&mut s, Punct::LParen);
        assert_next_is_punct(&mut s, Punct::RParen);
        assert!(s.next().is_none());

        let mut s = Tokenizer::new("<<>>");
        assert_next_is_punct(&mut s, Punct::Lt);
        assert_next_is_punct(&mut s, Punct::Lt);
        assert_next_is_punct(&mut s, Punct::GtGt);
        assert!(s.next().is_none());

        let mut s = Tokenizer::new(">>>");
        assert_next_is_punct(&mut s, Punct::GtGt);
        assert_next_is_punct(&mut s, Punct::Gt);
        assert!(s.next().is_none());
    }

    #[test]
    fn test_mixed_tokens() {
        let mut s = Tokenizer::new("echo hello | grep world > output.txt");
        assert_next_is_str(&mut s, "echo");
        assert_next_is_str(&mut s, "hello");
        assert_next_is_punct(&mut s, Punct::Pipe);
        assert_next_is_str(&mut s, "grep");
        assert_next_is_str(&mut s, "world");
        assert_next_is_punct(&mut s, Punct::Gt);
        assert_next_is_str(&mut s, "output.txt");
        assert!(s.next().is_none());

        let mut s = Tokenizer::new("cat file.txt && echo done");
        assert_next_is_str(&mut s, "cat");
        assert_next_is_str(&mut s, "file.txt");
        assert_next_is_punct(&mut s, Punct::AndAnd);
        assert_next_is_str(&mut s, "echo");
        assert_next_is_str(&mut s, "done");
        assert!(s.next().is_none());
    }

    #[test]
    fn test_empty_input() {
        let mut s = Tokenizer::new("");
        assert!(s.next().is_none());
    }

    #[test]
    fn test_whitespace_handling() {
        let mut s = Tokenizer::new("   echo   hello   ");
        assert_next_is_str(&mut s, "echo");
        assert_next_is_str(&mut s, "hello");
        assert!(s.next().is_none());

        let mut s = Tokenizer::new("   ");
        assert!(s.next().is_none());
    }
}
