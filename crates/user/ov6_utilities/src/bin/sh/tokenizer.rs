use alloc::{borrow::Cow, string::String};

const SYMBOLS: &[char] = &['<', '|', '>', '&', ';', '(', ')'];

#[derive(Debug, PartialEq, Eq)]
pub enum Token<'s> {
    Str(Cow<'s, str>),
    Punct(Punct),
}

impl PartialEq<str> for Token<'_> {
    fn eq(&self, other: &str) -> bool {
        match self {
            Token::Str(s) => s == other,
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

#[derive(Debug, PartialEq, Eq)]
pub enum Punct {
    Pipe,
    LParen,
    RParen,
    Semicolon,
    And,
    Lt,
    Gt,
    GtGt,
}

pub struct Tokenizer<'a> {
    input: &'a str,
}

impl<'a> Tokenizer<'a> {
    pub fn new(input: &'a str) -> Self {
        Self { input }
    }

    fn first(&self) -> Option<char> {
        self.input.chars().next()
    }

    fn skip(&mut self, n: usize) {
        let mut chars = self.input.chars();
        for _ in 0..n {
            if chars.next().is_none() {
                break;
            }
        }
        self.input = chars.as_str();
    }

    fn trim_start(&mut self) {
        self.input = self.input.trim_start();
    }

    fn next_char_if(&mut self, chars: &[char]) -> Option<char> {
        self.trim_start();
        let mut cs = self.input.chars();
        let c = cs.next()?;
        if !chars.contains(&c) {
            return None;
        }
        self.input = cs.as_str();
        Some(c)
    }

    fn next_str(&mut self) -> Token<'a> {
        let start = self.input;
        let mut in_double_quotes = false;
        let mut in_single_quotes = false;
        let mut escaped = false;
        let mut needs_allocation = false;

        // Counting phase
        while let Some(ch) = self.first() {
            if escaped {
                escaped = false;
                needs_allocation = true;
                self.skip(1);
                continue;
            }
            if ch == '\\' && !in_single_quotes {
                escaped = true;
                needs_allocation = true;
                self.skip(1);
                continue;
            }
            if ch == '\"' && !in_single_quotes {
                in_double_quotes = !in_double_quotes;
                needs_allocation = true;
                self.skip(1);
                continue;
            }
            if ch == '\'' && !in_double_quotes {
                in_single_quotes = !in_single_quotes;
                needs_allocation = true;
                self.skip(1);
                continue;
            }
            if !in_double_quotes
                && !in_single_quotes
                && (ch.is_whitespace() || SYMBOLS.contains(&ch))
            {
                break;
            }
            self.skip(1);
        }

        assert!(!escaped, "unterminated escape sequence");
        assert!(!in_double_quotes, "unterminated double quote");
        assert!(!in_single_quotes, "unterminated single quote");

        let input = &start[..start.len() - self.input.len()];
        self.trim_start();

        if !needs_allocation {
            return Token::Str(Cow::Borrowed(input));
        }

        let mut result = String::with_capacity(start.len() - self.input.len());
        let mut escaped = false;

        // Constructing the actual string
        for ch in input.chars() {
            if escaped {
                result.push(ch);
                escaped = false;
                continue;
            }
            if ch == '\\' && !in_single_quotes {
                escaped = true;
                continue;
            }
            if ch == '\"' && !in_single_quotes {
                in_double_quotes = !in_double_quotes;
                continue;
            }
            if ch == '\'' && !in_double_quotes {
                in_single_quotes = !in_single_quotes;
                continue;
            }
            result.push(ch);
        }

        Token::Str(Cow::Owned(result))
    }

    fn next_token(&mut self) -> Option<Token<'a>> {
        self.trim_start();
        let token = match self.first()? {
            '|' => {
                self.skip(1);
                Token::Punct(Punct::Pipe)
            }
            '(' => {
                self.skip(1);
                Token::Punct(Punct::LParen)
            }
            ')' => {
                self.skip(1);
                Token::Punct(Punct::RParen)
            }
            ';' => {
                self.skip(1);
                Token::Punct(Punct::Semicolon)
            }
            '&' => {
                self.skip(1);
                Token::Punct(Punct::And)
            }
            '<' => {
                self.skip(1);
                Token::Punct(Punct::Lt)
            }
            '>' => {
                self.skip(1);
                if self.next_char_if(&['>']).is_some() {
                    Token::Punct(Punct::GtGt)
                } else {
                    Token::Punct(Punct::Gt)
                }
            }
            _ => self.next_str(),
        };
        self.trim_start();
        Some(token)
    }
}

impl<'a> Iterator for Tokenizer<'a> {
    type Item = Token<'a>;

    fn next(&mut self) -> Option<Self::Item> {
        self.next_token()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_str() {
        let mut s = Tokenizer::new("hello world");
        assert_eq!(s.next(), Some(Token::Str("hello".into())));
        assert_eq!(s.next(), Some(Token::Str("world".into())));
        assert_eq!(s.next(), None);

        let mut s = Tokenizer::new(r#""hello world""#);
        assert_eq!(s.next(), Some(Token::Str("hello world".into())));
        assert_eq!(s.next(), None);

        let mut s = Tokenizer::new("'hello world'");
        assert_eq!(s.next(), Some(Token::Str("hello world".into())));
        assert_eq!(s.next(), None);

        let mut s = Tokenizer::new(r"hello\ world");
        assert_eq!(s.next(), Some(Token::Str("hello world".into())));
        assert_eq!(s.next(), None);
    }

    #[test]
    fn test_str_complex_cases() {
        let mut s = Tokenizer::new(r#"hello "world" 'test' \n \t \\"#);
        assert_eq!(s.next(), Some(Token::Str("hello".into())));
        assert_eq!(s.next(), Some(Token::Str("world".into())));
        assert_eq!(s.next(), Some(Token::Str("test".into())));
        assert_eq!(s.next(), Some(Token::Str("n".into())));
        assert_eq!(s.next(), Some(Token::Str("t".into())));
        assert_eq!(s.next(), Some(Token::Str("\\".into())));

        let mut s = Tokenizer::new(r#""hello \"world\"""#);
        assert_eq!(s.next(), Some(Token::Str("hello \"world\"".into())));

        let mut s = Tokenizer::new(r"'hello \'world\'''");
        assert_eq!(s.next(), Some(Token::Str("hello \\world\'".into())));

        let mut s = Tokenizer::new(r"hello\ world\!");
        assert_eq!(s.next(), Some(Token::Str("hello world!".into())));

        let mut s = Tokenizer::new(r#""hello\nworld""#);
        assert_eq!(s.next(), Some(Token::Str("hellonworld".into())));

        let mut s = Tokenizer::new(r"'hello\nworld'");
        assert_eq!(s.next(), Some(Token::Str(r"hello\nworld".into())));
    }

    #[test]
    #[should_panic = "unterminated escape sequence"]
    fn test_str_unterminated_escape() {
        let mut s = Tokenizer::new("hello\\");
        s.next_str();
    }

    #[test]
    #[should_panic = "unterminated double quote"]
    fn test_str_unterminated_double_quotes() {
        let mut s = Tokenizer::new("hello\"world");
        s.next_str();
    }

    #[test]
    #[should_panic = "unterminated single quote"]
    fn test_str_unterminated_single_quotes() {
        let mut s = Tokenizer::new("hello\'world");
        s.next_str();
    }

    #[test]
    fn test_punctuations() {
        let mut s = Tokenizer::new("|&;()");
        assert_eq!(s.next(), Some(Token::Punct(Punct::Pipe)));
        assert_eq!(s.next(), Some(Token::Punct(Punct::And)));
        assert_eq!(s.next(), Some(Token::Punct(Punct::Semicolon)));
        assert_eq!(s.next(), Some(Token::Punct(Punct::LParen)));
        assert_eq!(s.next(), Some(Token::Punct(Punct::RParen)));
        assert_eq!(s.next(), None);

        let mut s = Tokenizer::new("<<>>");
        assert_eq!(s.next(), Some(Token::Punct(Punct::Lt)));
        assert_eq!(s.next(), Some(Token::Punct(Punct::Lt)));
        assert_eq!(s.next(), Some(Token::Punct(Punct::GtGt)));
        assert_eq!(s.next(), None);

        let mut s = Tokenizer::new(">>>");
        assert_eq!(s.next(), Some(Token::Punct(Punct::GtGt)));
        assert_eq!(s.next(), Some(Token::Punct(Punct::Gt)));
        assert_eq!(s.next(), None);
    }

    #[test]
    fn test_mixed_tokens() {
        let mut s = Tokenizer::new("echo hello | grep world > output.txt");
        assert_eq!(s.next(), Some(Token::Str("echo".into())));
        assert_eq!(s.next(), Some(Token::Str("hello".into())));
        assert_eq!(s.next(), Some(Token::Punct(Punct::Pipe)));
        assert_eq!(s.next(), Some(Token::Str("grep".into())));
        assert_eq!(s.next(), Some(Token::Str("world".into())));
        assert_eq!(s.next(), Some(Token::Punct(Punct::Gt)));
        assert_eq!(s.next(), Some(Token::Str("output.txt".into())));
        assert_eq!(s.next(), None);

        let mut s = Tokenizer::new("cat file.txt && echo done");
        assert_eq!(s.next(), Some(Token::Str("cat".into())));
        assert_eq!(s.next(), Some(Token::Str("file.txt".into())));
        assert_eq!(s.next(), Some(Token::Punct(Punct::And)));
        assert_eq!(s.next(), Some(Token::Punct(Punct::And)));
        assert_eq!(s.next(), Some(Token::Str("echo".into())));
        assert_eq!(s.next(), Some(Token::Str("done".into())));
        assert_eq!(s.next(), None);
    }

    #[test]
    fn test_empty_input() {
        let mut s = Tokenizer::new("");
        assert_eq!(s.next(), None);
    }

    #[test]
    fn test_whitespace_handling() {
        let mut s = Tokenizer::new("   echo   hello   ");
        assert_eq!(s.next(), Some(Token::Str("echo".into())));
        assert_eq!(s.next(), Some(Token::Str("hello".into())));
        assert_eq!(s.next(), None);

        let mut s = Tokenizer::new("   ");
        assert_eq!(s.next(), None);
    }
}
