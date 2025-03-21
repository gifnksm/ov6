use alloc::{sync::Arc, vec};
use core::iter::Peekable;

use ov6_user_lib::sync::spin::Mutex;

use crate::{
    command::{Command, RedirectFd, RedirectMode},
    tokenizer::{Punct, Token, TokenizeError, Tokenizer},
};

macro_rules! try_opt {
    ($e:expr) => {
        match $e {
            Ok(Some(cmd)) => cmd,
            Ok(None) => return Ok(None),
            Err(e) => return Err(e),
        }
    };
}

#[derive(Debug, thiserror::Error)]
pub(super) enum ParseError {
    #[error(transparent)]
    Tokenize(#[from] TokenizeError),
    #[error("leftovers '{0}'")]
    Leftovers(Token<'static>),
    #[error("missing file for redirection")]
    MissingFile,
    #[error("missing '{0}'")]
    MissingPunct(Punct),
    #[error("unexpected charactesr '{0}'")]
    UnexpectedPunct(Punct),
}

struct PeekTokenizer<'a> {
    tokens: Peekable<Tokenizer<'a>>,
}

impl<'a> PeekTokenizer<'a> {
    fn new(input: &'a str) -> Self {
        Self {
            tokens: Tokenizer::new(input).peekable(),
        }
    }

    fn next(&mut self) -> Result<Option<Token<'a>>, TokenizeError> {
        self.tokens.next().transpose()
    }

    fn next_if<F>(&mut self, f: F) -> Result<Option<Token<'a>>, TokenizeError>
    where
        F: FnOnce(&Token<'a>) -> bool,
    {
        self.tokens
            .next_if(|t| match t {
                Ok(t) => f(t),
                Err(_e) => true,
            })
            .transpose()
    }
}

pub struct Parser<'a> {
    tokens: PeekTokenizer<'a>,
}

impl<'a> Parser<'a> {
    pub fn new(input: &'a str) -> Self {
        Self {
            tokens: PeekTokenizer::new(input),
        }
    }

    pub fn parse(&mut self) -> Result<Option<Command<'a>>, ParseError> {
        let cmd = try_opt!(self.parse_line());
        if let Some(token) = self.tokens.next()? {
            return Err(ParseError::Leftovers(token.into_owned()));
        }
        Ok(Some(cmd))
    }

    fn parse_line(&mut self) -> Result<Option<Command<'a>>, ParseError> {
        let mut cmd = try_opt!(self.parse_pipe());
        while self.tokens.next_if(|t| *t == Punct::And)?.is_some() {
            cmd = Command::Back { cmd: cmd.into() };
        }
        while self.tokens.next_if(|t| *t == Punct::Semicolon)?.is_some() {
            cmd = Command::List {
                left: cmd.into(),
                right: try_opt!(self.parse_line()).into(),
            };
        }
        Ok(Some(cmd))
    }

    fn parse_pipe(&mut self) -> Result<Option<Command<'a>>, ParseError> {
        let mut cmd = try_opt!(self.parse_exec());
        if self.tokens.next_if(|t| *t == Punct::Pipe)?.is_some() {
            cmd = Command::Pipe {
                left: cmd.into(),
                right: try_opt!(self.parse_pipe()).into(),
            };
        }
        Ok(Some(cmd))
    }

    fn parse_redirs(&mut self, mut cmd: Command<'a>) -> Result<Option<Command<'a>>, ParseError> {
        loop {
            let (mode, fd) = if self.tokens.next_if(|t| *t == Punct::Lt)?.is_some() {
                (RedirectMode::Input, RedirectFd::Stdin)
            } else if self.tokens.next_if(|t| *t == Punct::Gt)?.is_some() {
                (RedirectMode::OutputTrunc, RedirectFd::Stdout)
            } else if self.tokens.next_if(|t| *t == Punct::GtGt)?.is_some() {
                (RedirectMode::OutputAppend, RedirectFd::Stdout)
            } else {
                break;
            };
            let Some(Token::Str(file)) = self.tokens.next()? else {
                return Err(ParseError::MissingFile);
            };
            cmd = Command::Redirect {
                cmd: cmd.into(),
                file,
                mode,
                fd,
            };
        }
        Ok(Some(cmd))
    }

    fn parse_block(&mut self) -> Result<Option<Command<'a>>, ParseError> {
        // LParen is consumed by caller
        let mut cmd = try_opt!(self.parse_line());
        if self.tokens.next_if(|t| *t == Punct::RParen)?.is_none() {
            return Err(ParseError::MissingPunct(Punct::RParen));
        }
        cmd = try_opt!(self.parse_redirs(cmd));
        Ok(Some(cmd))
    }

    fn parse_exec(&mut self) -> Result<Option<Command<'a>>, ParseError> {
        if self.tokens.next_if(|t| *t == Punct::LParen)?.is_some() {
            return self.parse_block();
        }

        let argv = Arc::new(Mutex::new(vec![]));
        let mut cmd = Command::Exec {
            argv: Arc::clone(&argv),
        };

        cmd = try_opt!(self.parse_redirs(cmd));
        while let Some(tok) = self.tokens.next_if(|t| {
            *t != Punct::Pipe && *t != Punct::RParen && *t != Punct::And && *t != Punct::Semicolon
        })? {
            let arg = match tok {
                Token::Str(arg) => arg,
                Token::Punct(p) => return Err(ParseError::UnexpectedPunct(p)),
            };
            argv.lock().push(arg);
            cmd = try_opt!(self.parse_redirs(cmd));
        }
        if argv.lock().is_empty() {
            return Ok(None);
        }
        Ok(Some(cmd))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(input: &str) -> Result<Option<Command>, ParseError> {
        let cmd = try_opt!(Parser::new(input).parse());
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
