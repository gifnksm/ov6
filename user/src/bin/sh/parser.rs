use core::fmt;

use alloc::{format, string::String, sync::Arc};
use xv6_user_lib::{
    fs::OpenFlags,
    io::{STDIN_FD, STDOUT_FD},
    sync::spin::Mutex,
};

use crate::command::{Command, MAX_ARGS};

const SYMBOLS: &[char] = &['<', '|', '>', '&', ';', '(', ')'];

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
            while first(s)
                .map(|ch| !ch.is_whitespace() && !SYMBOLS.contains(&ch))
                .unwrap_or(false)
            {
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

pub(super) fn parse_cmd<'a>(s: &mut &'a str) -> Result<Command<'a>, ParseError> {
    let cmd = parse_line(s)?;
    trim_start(s);
    if !s.is_empty() {
        return Err(format!("leftover: {s:?}").into());
    }
    Ok(cmd)
}

fn parse_line<'a>(s: &mut &'a str) -> Result<Command<'a>, ParseError> {
    let mut cmd = parse_pipe(s)?;
    while consume_char(s, &['&']).is_some() {
        cmd = Command::Back { cmd: cmd.into() };
    }
    while consume_char(s, &[';']).is_some() {
        cmd = Command::List {
            left: cmd.into(),
            right: parse_line(s)?.into(),
        };
    }
    Ok(cmd)
}

fn parse_pipe<'a>(s: &mut &'a str) -> Result<Command<'a>, ParseError> {
    let mut cmd = parse_exec(s)?;
    if consume_char(s, &['|']).is_some() {
        cmd = Command::Pipe {
            left: cmd.into(),
            right: parse_pipe(s)?.into(),
        };
    }
    Ok(cmd)
}

fn parse_redirs<'a>(mut cmd: Command<'a>, s: &mut &'a str) -> Result<Command<'a>, ParseError> {
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
                mode: OpenFlags::READ_ONLY,
                fd: STDIN_FD,
                fd_name: "stdin",
            },
            '>' => Command::Redirect {
                cmd: cmd.into(),
                file,
                mode: OpenFlags::WRITE_ONLY | OpenFlags::CREATE | OpenFlags::TRUNC,
                fd: STDOUT_FD,
                fd_name: "stdout",
            },
            '+' => Command::Redirect {
                cmd: cmd.into(),
                file,
                mode: OpenFlags::WRITE_ONLY | OpenFlags::CREATE,
                fd: STDOUT_FD,
                fd_name: "stdout",
            },
            _ => unreachable!(),
        }
    }
    Ok(cmd)
}

fn parse_block<'a>(s: &mut &'a str) -> Result<Command<'a>, ParseError> {
    consume_char(s, &['(']).unwrap();
    let mut cmd = parse_line(s)?;
    if consume_char(s, &[')']).is_none() {
        return Err(r#"missing ")""#.into());
    }
    cmd = parse_redirs(cmd, s)?;
    Ok(cmd)
}

fn parse_exec<'a>(s: &mut &'a str) -> Result<Command<'a>, ParseError> {
    if peek_char(s, &['(']).is_some() {
        return parse_block(s);
    }

    let argv = Arc::new(Mutex::new([const { None }; MAX_ARGS]));
    let mut cmd = Command::Exec {
        argv: Arc::clone(&argv),
    };

    let mut argc = 0;
    cmd = parse_redirs(cmd, s)?;
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
        cmd = parse_redirs(cmd, s)?;
    }
    Ok(cmd)
}
