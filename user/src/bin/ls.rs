#![no_std]

use ov6_user_lib::{
    env,
    fs::{self, Metadata, StatType},
    os_str::OsStr,
    path::Path,
    println, process,
};
use user::try_or;

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
    )
}

fn ls<P>(path: P)
where
    P: AsRef<Path>,
{
    let path = path.as_ref();
    let meta = try_or!(
        fs::metadata(path),
        return,
        e => "cannot stat {}: {e}", path.to_str().unwrap(),
    );

    match meta.ty() {
        StatType::File | StatType::Dev => print_entry(OsStr::new(path.to_str().unwrap()), &meta),
        StatType::Dir => {
            let entries = try_or!(
                fs::read_dir(path),
                return,
                e => "cannot open {} as directory: {e}", path.to_str().unwrap(),
            );
            for ent in entries {
                let ent = try_or!(
                    ent,
                    return,
                    e => "cannot read directory entry: {e}",
                );
                let name = ent.name();
                let file_path = path.join(name);
                let meta = try_or!(
                    fs::metadata(&file_path),
                    continue,
                    e => "cannot stat {}: {e}", file_path.display(),
                );
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
