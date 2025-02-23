#![no_std]

use xv6_user_lib::{
    env, eprintln,
    fs::{File, OpenFlags},
    io::{self, Read},
    println, process,
};

fn grep<R>(pattern: &str, mut input: R, buf: &mut [u8])
where
    R: Read,
{
    let prog = env::arg0();
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
            let Ok(line) = str::from_utf8(line) else {
                panic!("{prog}: invalid utf-8");
            };
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

    let prog = env::arg0();
    let mut args = env::args_cstr();

    let Some(pattern) = args.next() else {
        eprintln!("Usage: {prog} pattern [file...]");
        process::exit(1);
    };

    let Ok(pattern) = pattern.to_str() else {
        eprintln!("{prog}: invalid utf-8");
        process::exit(1);
    };

    if args.len() == 0 {
        let stdin = io::stdin();
        grep(pattern, stdin, &mut buf);
    } else {
        for arg in args {
            let Ok(file) = File::open(arg, OpenFlags::READ_ONLY) else {
                eprintln!("{prog}: cannot open {}", arg.to_str().unwrap());
                continue;
            };
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
