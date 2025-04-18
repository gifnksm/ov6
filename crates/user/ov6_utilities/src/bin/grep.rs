#![no_std]

use ov6_user_lib::{
    env,
    fs::File,
    io::{self, Read},
    println,
};
use ov6_utilities::{OrExit as _, exit_err, message_err, usage_and_exit};

fn grep<R>(pattern: &str, mut input: R, buf: &mut [u8])
where
    R: Read,
{
    let mut filled = 0;

    loop {
        let Ok(n) = input.read(&mut buf[filled..]) else {
            return;
        };
        filled += n;

        if filled == 0 {
            return;
        }

        let mut consumed = 0;
        while let Some(i) = buf[consumed..filled].iter().position(|c| *c == b'\n') {
            let line = &buf[consumed..filled][..i];
            let line = str::from_utf8(line).or_exit(|e| exit_err!(e, "parse line error"));
            if match_(pattern, line) {
                println!("{}", line);
            }
            consumed += i + 1;
        }

        buf.copy_within(consumed..filled, 0);
        filled -= consumed;
    }
}

fn main() {
    let mut buf = [0; 1024];

    let mut args = env::args_os();
    let _ = args.next(); // skip the program name

    let Some(pattern) = args.next() else {
        usage_and_exit!("pattern [file...]");
    };

    let Some(pattern) = pattern.to_str() else {
        usage_and_exit!("pattern must be valid UTF-8");
    };

    if args.len() == 0 {
        let stdin = io::stdin();
        grep(pattern, stdin, &mut buf);
    } else {
        let files = args.flat_map(|arg| {
            File::open(arg).inspect_err(|e| message_err!(e, "cannot open '{}'", arg.display()))
        });
        for file in files {
            grep(pattern, file, &mut buf);
        }
    }
}

// Regexp matcher from Kernighan & Pike,
// The Practice of Programming, Chapter 9, or
// https://www.cs.princeton.edu/courses/archive/spr09/cos333/beautiful.html

fn match_(re: &str, text: &str) -> bool {
    if let Some(re) = re.strip_prefix('^') {
        return match_here(re, text);
    }

    for (i, _) in text.char_indices() {
        if match_here(re, &text[i..]) {
            return true;
        }
    }

    if match_here(re, "") {
        return true;
    }

    false
}

fn split_first_char(s: &str) -> Option<(char, &str)> {
    let mut cs = s.chars();
    let ch = cs.next()?;
    Some((ch, cs.as_str()))
}

/// search for `re` at beginning of text
fn match_here(re: &str, text: &str) -> bool {
    let Some((re_next, re_rest)) = split_first_char(re) else {
        // if re is empty, returns true
        return true;
    };
    if let Some(re_rest) = re_rest.strip_prefix('*') {
        return match_star(re_next, re_rest, text);
    }
    if re_next == '$' {
        return text.is_empty();
    }
    if let Some((text_next, text_rest)) = split_first_char(text) {
        return (re_next == '.' || re_next == text_next) && match_here(re_rest, text_rest);
    }
    false
}

// search for `c*re` at beginning of text
fn match_star(c: char, re: &str, text: &str) -> bool {
    let mut t = text;
    loop {
        if match_here(re, t) {
            return true;
        }
        if t.is_empty() || (c != '.' && !t.starts_with(c)) {
            return false;
        }
        t = &t[1..];
    }
}
