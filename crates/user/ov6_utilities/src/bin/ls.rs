#![no_std]

use ov6_user_lib::{
    env,
    fs::{self, Metadata, StatType},
    os_str::OsStr,
    path::Path,
    println, process,
};
use ov6_utilities::message_err;

fn print_entry(name: &OsStr, meta: &Metadata) {
    let ty = match meta.ty() {
        StatType::Dir => "dir",
        StatType::File => "file",
        StatType::Dev => "dev",
    };
    println!(
        "{:16} {:4} {:6} {:12}",
        name.display(),
        ty,
        meta.ino(),
        meta.size(),
    );
}

fn ls<P>(path: P)
where
    P: AsRef<Path>,
{
    let path = path.as_ref();
    let Ok(meta) =
        fs::metadata(path).inspect_err(|e| message_err!(e, "cannot stat '{}'", path.display()))
    else {
        return;
    };

    match meta.ty() {
        StatType::File | StatType::Dev => print_entry(OsStr::new(path.to_str().unwrap()), &meta),
        StatType::Dir => {
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
                let name = ent.name();
                let file_path = path.join(name);
                let Ok(meta) = fs::metadata(&file_path)
                    .inspect_err(|e| message_err!(e, "cannot stat '{}'", file_path.display()))
                else {
                    continue;
                };
                print_entry(name, &meta);
            }
        }
    }
}

fn main() {
    let args = env::args_os();

    if args.len() < 1 {
        ls(".");
        process::exit(0);
    }
    for arg in args {
        ls(arg);
    }
    process::exit(0);
}
