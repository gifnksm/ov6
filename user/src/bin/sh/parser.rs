use alloc::{format, string::String, sync::Arc};
use core::fmt;

use ov6_user_lib::sync::spin::Mutex;

use crate::command::{Command, MAX_ARGS, RedirectFd, RedirectMode};

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

fn consume_one(s: &mut &str) -> Option<char> {
    trim_start(s);
    let mut cs = s.chars();
    let ch = cs.next()?;
    *s = cs.as_str();
    Some(ch)
}

fn consume_char(s: &mut &str, chars: &[char]) -> Option<char> {
    trim_start(s);
    let rest = s.strip_prefix(chars)?;
    let stripped = first(&s[..s.len() - rest.len()]).unwrap();
    *s = rest;
    Some(stripped)
}

enum Token<'s> {
    Str(&'s str),
    Punct(char),
}

fn consume_token<'s>(s: &mut &'s str) -> Option<Token<'s>> {
    trim_start(s);
    let start = *s;
    let token = match consume_one(s)? {
        ch @ ('|' | '(' | ')' | ';' | '&' | '<') => Token::Punct(ch),
        '>' => {
            if consume_char(s, &['>']).is_some() {
                Token::Punct('+')
            } else {
                Token::Punct('>')
            }
        }
        _ => {
            while first(s).is_some_and(|ch| !ch.is_whitespace() && !SYMBOLS.contains(&ch)) {
                skip(s, 1);
            }
            let end = *s;
            let qlen = start.len() - end.len();
            Token::Str(&start[..qlen])
        }
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

    let argv = Arc::new(Mutex::new([const { None }; MAX_ARGS]));
    let mut cmd = Command::Exec {
        argv: Arc::clone(&argv),
    };

    let mut argc = 0;
    cmd = try_opt!(parse_redirs(cmd, s));
    while peek_char(s, &['|', ')', '&', ';']).is_none() {
        let Some(tok) = consume_token(s) else {
            break;
        };
        let arg = match tok {
            Token::Str(arg) => arg,
            Token::Punct(p) => return Err(format!("unexpected character {p:?}").into()),
        };
        argv.lock()[argc] = Some(arg);
        argc += 1;
        if argc >= MAX_ARGS {
            return Err("too many arguments".into());
        }
        cmd = try_opt!(parse_redirs(cmd, s));
    }
    if argc == 0 {
        return Ok(None);
    }
    Ok(Some(cmd))
}

#[cfg(test)]
mod tests {
    use super::*;

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
        if let Command::Exec { argv } = cmd {
            let args = argv.lock();
            assert_eq!(args[0], Some("echo"));
            assert_eq!(args[1], Some("hello"));
            assert_eq!(args[2], None);
        } else {
            panic!("Expected Exec command");
        }
    }

    #[test]
    fn test_parse_pipe() {
        let cmd = parse("echo hello | grep h").unwrap().unwrap();
        if let Command::Pipe { left, right } = cmd {
            if let Command::Exec { argv } = *left {
                let args = argv.lock();
                assert_eq!(args[0], Some("echo"));
                assert_eq!(args[1], Some("hello"));
                assert_eq!(args[2], None);
            } else {
                panic!("Expected Exec command on the left side of the pipe");
            }
            if let Command::Exec { argv } = *right {
                let args = argv.lock();
                assert_eq!(args[0], Some("grep"));
                assert_eq!(args[1], Some("h"));
                assert_eq!(args[2], None);
            } else {
                panic!("Expected Exec command on the right side of the pipe");
            }
        } else {
            panic!("Expected Pipe command");
        }
    }

    #[test]
    fn test_parse_redirection() {
        let cmd = parse("echo hello > output.txt").unwrap().unwrap();
        if let Command::Redirect {
            cmd,
            file,
            mode,
            fd,
        } = cmd
        {
            assert_eq!(file, "output.txt");
            assert_eq!(mode, RedirectMode::OutputTrunc);
            assert_eq!(fd, RedirectFd::Stdout);
            if let Command::Exec { argv } = *cmd {
                let args = argv.lock();
                assert_eq!(args[0], Some("echo"));
                assert_eq!(args[1], Some("hello"));
                assert_eq!(args[2], None);
            } else {
                panic!("Expected Exec command");
            }
        } else {
            panic!("Expected Redirect command");
        }
    }

    #[test]
    fn test_parse_background() {
        let cmd = parse("sleep 1 &").unwrap().unwrap();
        if let Command::Back { cmd } = cmd {
            if let Command::Exec { argv } = *cmd {
                let args = argv.lock();
                assert_eq!(args[0], Some("sleep"));
                assert_eq!(args[1], Some("1"));
                assert_eq!(args[2], None);
            } else {
                panic!("Expected Exec command");
            }
        } else {
            panic!("Expected Back command");
        }
    }

    #[test]
    fn test_parse_list() {
        let cmd = parse("echo hello; echo world").unwrap().unwrap();
        if let Command::List { left, right } = cmd {
            if let Command::Exec { argv } = *left {
                let args = argv.lock();
                assert_eq!(args[0], Some("echo"));
                assert_eq!(args[1], Some("hello"));
                assert_eq!(args[2], None);
            } else {
                panic!("Expected Exec command on the left side of the list");
            }
            if let Command::Exec { argv } = *right {
                let args = argv.lock();
                assert_eq!(args[0], Some("echo"));
                assert_eq!(args[1], Some("world"));
                assert_eq!(args[2], None);
            } else {
                panic!("Expected Exec command on the right side of the list");
            }
        } else {
            panic!("Expected List command");
        }
    }

    #[test]
    fn test_parse_nested_commands() {
        let cmd = parse("(echo hello; echo world) | grep h").unwrap().unwrap();
        if let Command::Pipe { left, right } = cmd {
            if let Command::List {
                left: list_left,
                right: list_right,
            } = *left
            {
                if let Command::Exec { argv } = *list_left {
                    let args = argv.lock();
                    assert_eq!(args[0], Some("echo"));
                    assert_eq!(args[1], Some("hello"));
                    assert_eq!(args[2], None);
                } else {
                    panic!("Expected Exec command on the left side of the list");
                }
                if let Command::Exec { argv } = *list_right {
                    let args = argv.lock();
                    assert_eq!(args[0], Some("echo"));
                    assert_eq!(args[1], Some("world"));
                    assert_eq!(args[2], None);
                } else {
                    panic!("Expected Exec command on the right side of the list");
                }
            } else {
                panic!("Expected List command on the left side of the pipe");
            }
            if let Command::Exec { argv } = *right {
                let args = argv.lock();
                assert_eq!(args[0], Some("grep"));
                assert_eq!(args[1], Some("h"));
                assert_eq!(args[2], None);
            } else {
                panic!("Expected Exec command on the right side of the pipe");
            }
        } else {
            panic!("Expected Pipe command");
        }
    }
}
