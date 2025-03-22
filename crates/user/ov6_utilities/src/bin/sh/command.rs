use alloc::{borrow::Cow, boxed::Box, vec::Vec};

use ov6_user_lib::os_str::OsStr;

#[derive(Debug, Default)]
pub(super) struct Redirect<'a> {
    pub(super) stdin: Option<Cow<'a, str>>,
    pub(super) stdout: Option<(Cow<'a, str>, OutputMode)>,
}

impl Redirect<'_> {
    pub(super) fn new() -> Self {
        Self::default()
    }
}

#[derive(Debug)]
pub(super) enum CommandKind<'a> {
    Subshell {
        list: Vec<Command<'a>>,
        redirect: Redirect<'a>,
    },
    Exec {
        argv: Vec<Cow<'a, OsStr>>,
        redirect: Redirect<'a>,
    },
    Pipe {
        left: Command<'a>,
        right: Command<'a>,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum OutputMode {
    Truncate,
    Append,
}

#[derive(Debug)]
pub(super) struct Command<'a> {
    pub(super) kind: Box<CommandKind<'a>>,
    pub(super) background: bool,
}

impl<'a> Command<'a> {
    fn new(kind: CommandKind<'a>) -> Self {
        Self {
            kind: Box::new(kind),
            background: false,
        }
    }

    pub(super) fn subshell(list: Vec<Self>, redirect: Redirect<'a>) -> Self {
        Self::new(CommandKind::Subshell { list, redirect })
    }

    pub(super) fn exec(argv: Vec<Cow<'a, OsStr>>, redirect: Redirect<'a>) -> Self {
        Self::new(CommandKind::Exec { argv, redirect })
    }

    pub(super) fn pipe(left: Self, right: Self) -> Self {
        Self::new(CommandKind::Pipe { left, right })
    }
}
