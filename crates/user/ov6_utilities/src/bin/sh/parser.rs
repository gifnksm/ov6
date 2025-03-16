use alloc::{borrow::Cow, format, string::String, sync::Arc, vec};
use core::fmt;

use ov6_user_lib::sync::spin::Mutex;

use crate::command::{Command, RedirectFd, RedirectMode};

const SYMBOLS: &[char] = &['<', '|', '>', '&', ';', '(', ')'];

macro_rules! try_opt {
    ($e:expr) => {
        match $e {
            Ok(Some(cmd)) => cmd,
            Ok(None) => return Ok(None),
            Err(e) => return Err(e.into()),
        }
    };
}

#[derive(Debug)]
pub(super) struct ParseError {
    msg: String,
}

impl ParseError {
    fn new(msg: impl Into<String>) -> Self {
        Self { msg: msg.into() }
    }
}

impl fmt::Display for ParseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Display::fmt(&self.msg, f)
    }
}

impl core::error::Error for ParseError {}

impl<T> From<T> for ParseError
where
    T: Into<String>,
{
    fn from(msg: T) -> Self {
        Self::new(msg)
    }
}

fn first(s: &str) -> Option<char> {
    s.chars().next()
}

fn skip(s: &mut &str, n: usize) {
    let mut cs = s.chars();
    for _ in 0..n {
        if cs.next().is_none() {
            break;
        }
    }
    *s = cs.as_str();
}

fn trim_start(s: &mut &str) {
    *s = s.trim_start();
}

fn peek_char(s: &mut &str, chars: &[char]) -> Option<char> {
    trim_start(s);
    first(s).filter(|ch| chars.contains(ch))
}

fn consume_char(s: &mut &str, chars: &[char]) -> Option<char> {
    trim_start(s);
    let rest = s.strip_prefix(chars)?;
    let stripped = first(&s[..s.len() - rest.len()]).unwrap();
    *s = rest;
    Some(stripped)
}

fn consume_str<'s>(s: &mut &'s str) -> Token<'s> {
    let start = *s;
    let mut in_double_quotes = false;
    let mut in_single_quotes = false;
    let mut escaped = false;
    let mut needs_allocation = false;

    // Counting phase
    while let Some(ch) = first(s) {
        if escaped {
            escaped = false;
            needs_allocation = true;
            skip(s, 1);
            continue;
        }
        if ch == '\\' && !in_single_quotes {
            escaped = true;
            needs_allocation = true;
            skip(s, 1);
            continue;
        }
        if ch == '\"' && !in_single_quotes {
            in_double_quotes = !in_double_quotes;
            needs_allocation = true;
            skip(s, 1);
            continue;
        }
        if ch == '\'' && !in_double_quotes {
            in_single_quotes = !in_single_quotes;
            needs_allocation = true;
            skip(s, 1);
            continue;
        }
        if !in_double_quotes && !in_single_quotes && (ch.is_whitespace() || SYMBOLS.contains(&ch)) {
            break;
        }
        skip(s, 1);
    }

    assert!(!escaped, "unterminated escape sequence");
    assert!(!in_double_quotes, "unterminated double quote");
    assert!(!in_single_quotes, "unterminated single quote");

    let input = &start[..start.len() - s.len()];
    trim_start(s);

    if !needs_allocation {
        return Token::Str(Cow::Borrowed(input));
    }

    let mut result = String::with_capacity(start.len() - s.len());
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

#[derive(Debug)]
enum Token<'s> {
    Str(Cow<'s, str>),
    Punct(char),
}

fn consume_token<'s>(s: &mut &'s str) -> Option<Token<'s>> {
    trim_start(s);
    let token = match first(s)? {
        ch @ ('|' | '(' | ')' | ';' | '&' | '<') => {
            skip(s, 1);
            Token::Punct(ch)
        }
        '>' => {
            skip(s, 1);
            if consume_char(s, &['>']).is_some() {
                Token::Punct('+')
            } else {
                Token::Punct('>')
            }
        }
        _ => consume_str(s),
    };
    trim_start(s);
    Some(token)
}

pub(super) fn parse_cmd<'a>(s: &mut &'a str) -> Result<Option<Command<'a>>, ParseError> {
    let cmd = try_opt!(parse_line(s));
    trim_start(s);
    if !s.is_empty() {
        return Err(format!("leftover: {s:?}").into());
    }
    Ok(Some(cmd))
}

fn parse_line<'a>(s: &mut &'a str) -> Result<Option<Command<'a>>, ParseError> {
    let mut cmd = try_opt!(parse_pipe(s));
    while consume_char(s, &['&']).is_some() {
        cmd = Command::Back { cmd: cmd.into() };
    }
    while consume_char(s, &[';']).is_some() {
        cmd = Command::List {
            left: cmd.into(),
            right: try_opt!(parse_line(s)).into(),
        };
    }
    Ok(Some(cmd))
}

fn parse_pipe<'a>(s: &mut &'a str) -> Result<Option<Command<'a>>, ParseError> {
    let mut cmd = try_opt!(parse_exec(s));
    if consume_char(s, &['|']).is_some() {
        cmd = Command::Pipe {
            left: cmd.into(),
            right: try_opt!(parse_pipe(s)).into(),
        };
    }
    Ok(Some(cmd))
}

fn parse_redirs<'a>(
    mut cmd: Command<'a>,
    s: &mut &'a str,
) -> Result<Option<Command<'a>>, ParseError> {
    while peek_char(s, &['<', '>']).is_some() {
        let Some(Token::Punct(tok)) = consume_token(s) else {
            unreachable!()
        };
        let Some(Token::Str(file)) = consume_token(s) else {
            return Err("missing file for redirection".into());
        };
        cmd = match tok {
            '<' => Command::Redirect {
                cmd: cmd.into(),
                file,
                mode: RedirectMode::Input,
                fd: RedirectFd::Stdin,
            },
            '>' => Command::Redirect {
                cmd: cmd.into(),
                file,
                mode: RedirectMode::OutputTrunc,
                fd: RedirectFd::Stdout,
            },
            '+' => Command::Redirect {
                cmd: cmd.into(),
                file,
                mode: RedirectMode::OutputAppend,
                fd: RedirectFd::Stdout,
            },
            _ => unreachable!(),
        }
    }
    Ok(Some(cmd))
}

fn parse_block<'a>(s: &mut &'a str) -> Result<Option<Command<'a>>, ParseError> {
    consume_char(s, &['(']).unwrap();
    let mut cmd = try_opt!(parse_line(s));
    if consume_char(s, &[')']).is_none() {
        return Err(r#"missing ")""#.into());
    }
    cmd = try_opt!(parse_redirs(cmd, s));
    Ok(Some(cmd))
}

fn parse_exec<'a>(s: &mut &'a str) -> Result<Option<Command<'a>>, ParseError> {
    if peek_char(s, &['(']).is_some() {
        return parse_block(s);
    }

    let argv = Arc::new(Mutex::new(vec![]));
    let mut cmd = Command::Exec {
        argv: Arc::clone(&argv),
    };

    cmd = try_opt!(parse_redirs(cmd, s));
    while peek_char(s, &['|', ')', '&', ';']).is_none() {
        let Some(tok) = consume_token(s) else {
            break;
        };
        let arg = match tok {
            Token::Str(arg) => arg,
            Token::Punct(p) => return Err(format!("unexpected character {p:?}").into()),
        };
        argv.lock().push(arg);
        cmd = try_opt!(parse_redirs(cmd, s));
    }
    if argv.lock().is_empty() {
        return Ok(None);
    }
    Ok(Some(cmd))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[track_caller]
    fn expect_str(s: &mut &str, expected: &str) {
        let token = consume_str(s);
        let Token::Str(token) = token else {
            panic!("unexpected: {token:?}");
        };
        assert_eq!(token, expected);
    }

    #[test]
    fn test_consume_str() {
        let mut s = "hello world";
        expect_str(&mut s, "hello");
        expect_str(&mut s, "world");

        let mut s = r#""hello world""#;
        expect_str(&mut s, "hello world");

        let mut s = "'hello world'";
        expect_str(&mut s, "hello world");

        let mut s = r"hello\ world";
        expect_str(&mut s, "hello world");
    }

    #[test]
    fn test_consume_str_complex_cases() {
        let mut s = r#"hello "world" 'test' \n \t \\"#;
        expect_str(&mut s, "hello");
        expect_str(&mut s, "world");
        expect_str(&mut s, "test");
        expect_str(&mut s, "n");
        expect_str(&mut s, "t");
        expect_str(&mut s, "\\");

        let mut s = r#""hello \"world\"""#;
        expect_str(&mut s, "hello \"world\"");

        let mut s = r"'hello \'world\'''";
        expect_str(&mut s, "hello \\world\'");

        let mut s = r"hello\ world\!";
        expect_str(&mut s, "hello world!");

        let mut s = r#""hello\nworld""#;
        expect_str(&mut s, "hellonworld");

        let mut s = r"'hello\nworld'";
        expect_str(&mut s, r"hello\nworld");
    }

    #[test]
    #[should_panic = "unterminated escape sequence"]
    fn test_consume_str_unterminated_escape() {
        let mut s = "hello\\";
        consume_str(&mut s);
    }

    #[test]
    #[should_panic = "unterminated double quote"]
    fn test_consume_str_unterminated_double_quotes() {
        let mut s = "hello\"world";
        consume_str(&mut s);
    }

    #[test]
    #[should_panic = "unterminated single quote"]
    fn test_consume_str_unterminated_single_quotes() {
        let mut s = "hello\'world";
        consume_str(&mut s);
    }

    fn parse(input: &str) -> Result<Option<Command>, ParseError> {
        let mut s = input;
        let cmd = try_opt!(parse_cmd(&mut s));
        assert!(s.is_empty());
        Ok(Some(cmd))
    }

    #[test]
    fn test_parse_empty() {
        assert!(parse("").unwrap().is_none());
        assert!(parse("   ").unwrap().is_none());
    }

    #[test]
    fn test_parse_simple_command() {
        let cmd = parse("echo hello").unwrap().unwrap();
        let Command::Exec { argv } = cmd else {
            panic!("Expected Exec command");
        };
        assert_eq!(&*argv.lock(), &["echo", "hello"]);
    }

    #[test]
    fn test_parse_pipe() {
        let cmd = parse("echo hello | grep h").unwrap().unwrap();
        let Command::Pipe { left, right } = cmd else {
            panic!("Expected Pipe command");
        };

        let Command::Exec { argv } = *left else {
            panic!("Expected Exec command on the left side of the pipe");
        };
        assert_eq!(&*argv.lock(), &["echo", "hello"]);
        let Command::Exec { argv } = *right else {
            panic!("Expected Exec command on the right side of the pipe");
        };

        let args = argv.lock();
        assert_eq!(&*args, &["grep", "h"]);
    }

    #[test]
    fn test_parse_redirection() {
        let cmd = parse("echo hello > output.txt").unwrap().unwrap();
        let Command::Redirect {
            cmd,
            file,
            mode,
            fd,
        } = cmd
        else {
            panic!("Expected Redirect command");
        };
        assert_eq!(file, "output.txt");
        assert_eq!(mode, RedirectMode::OutputTrunc);
        assert_eq!(fd, RedirectFd::Stdout);
        let Command::Exec { argv } = *cmd else {
            panic!("Expected Exec command");
        };
        assert_eq!(&*argv.lock(), &["echo", "hello"]);
    }

    #[test]
    fn test_parse_background() {
        let cmd = parse("sleep 1 &").unwrap().unwrap();
        let Command::Back { cmd } = cmd else {
            panic!("Expected Back command");
        };
        let Command::Exec { argv } = *cmd else {
            panic!("Expected Exec command");
        };
        assert_eq!(&*argv.lock(), &["sleep", "1"]);
    }

    #[test]
    fn test_parse_list() {
        let cmd = parse("echo hello; echo world").unwrap().unwrap();
        let Command::List { left, right } = cmd else {
            panic!("Expected List command");
        };
        let Command::Exec { argv } = *left else {
            panic!("Expected Exec command on the left side of the list");
        };
        assert_eq!(&*argv.lock(), &["echo", "hello"]);
        let Command::Exec { argv } = *right else {
            panic!("Expected Exec command on the right side of the list");
        };
        assert_eq!(&*argv.lock(), &["echo", "world"]);
    }

    #[test]
    fn test_parse_nested_commands() {
        let cmd = parse("(echo hello; echo world) | grep h").unwrap().unwrap();
        let Command::Pipe { left, right } = cmd else {
            panic!("Expected Pipe command");
        };
        let Command::List {
            left: list_left,
            right: list_right,
        } = *left
        else {
            panic!("Expected List command on the left side of the pipe");
        };
        let Command::Exec { argv } = *list_left else {
            panic!("Expected Exec command on the left side of the list");
        };
        assert_eq!(&*argv.lock(), &["echo", "hello"]);
        let Command::Exec { argv } = *list_right else {
            panic!("Expected Exec command on the right side of the list");
        };
        assert_eq!(&*argv.lock(), &["echo", "world"]);
        let Command::Exec { argv } = *right else {
            panic!("Expected Exec command on the right side of the pipe");
        };
        assert_eq!(&*argv.lock(), &["grep", "h"]);
    }

    #[test]
    fn test_parse_multiple_pipes() {
        let cmd = parse("echo hello | grep h | wc -l").unwrap().unwrap();
        let Command::Pipe { left, right } = cmd else {
            panic!("Expected Pipe command");
        };

        let Command::Exec { argv } = *left else {
            panic!("Expected Exec command on the right side of the second pipe");
        };
        assert_eq!(&*argv.lock(), &["echo", "hello"]);

        let Command::Pipe {
            left: right_left,
            right,
        } = *right
        else {
            panic!("Expected Pipe command on the left side of the second pipe");
        };

        let Command::Exec { argv } = *right_left else {
            panic!("Expected Exec command on the left side of the first pipe");
        };
        assert_eq!(&*argv.lock(), &["grep", "h"]);

        let Command::Exec { argv } = *right else {
            panic!("Expected Exec command on the right side of the first pipe");
        };
        assert_eq!(&*argv.lock(), &["wc", "-l"]);
    }
}
