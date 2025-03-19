#![no_std]

use ov6_user_lib::{env, fs, path::Path, println};
use ov6_utilities::message_err;

fn find(path: &Path, pattern: &Path) {
    let Ok(meta) =
        fs::metadata(path).inspect_err(|e| message_err!(e, "cannot stat '{}'", path.display()))
    else {
        return;
    };

    if path.ends_with(pattern) {
        println!("{}", path.display());
    }

    if !meta.is_dir() {
        return;
    }

    let Ok(entries) = fs::read_dir(path)
        .inspect_err(|e| message_err!(e, "cannot open '{}' as directory", path.display()))
    else {
        return;
    };

    let entries = entries.flat_map(|ent| {
        ent.inspect_err(|e| {
            message_err!(e, "cannot read directory '{}' entry", path.display());
        })
    });
    for ent in entries {
        find(&path.join(ent.name()), pattern);
    }
}

fn main() {
    let mut args = env::args_os();
    match args.len() {
        0 => find(".".as_ref(), "".as_ref()),
        1 => find(args.next().unwrap().as_ref(), "".as_ref()),
        _ => {
            let pattern = args.next_back().unwrap();
            for path in args {
                find(path.as_ref(), pattern.as_ref());
            }
        }
    }
}
