#![no_std]

use core::ffi::CStr;

use ov6_user_lib::{
    env,
    fs::{self, Metadata, StatType},
    println, process,
};
use user::{ensure_or, try_or};

fn print_entry(name: &str, meta: &Metadata) {
    let ty = match meta.ty() {
        StatType::Dir => "dir",
        StatType::File => "file",
        StatType::Dev => "dev",
    };
    println!("{:16} {:4} {:6} {:12}", name, ty, meta.ino(), meta.size(),)
}

fn ls(path: &CStr) {
    let meta = try_or!(
        fs::metadata(path),
        return,
        e => "cannot stat {}: {e}", path.to_str().unwrap(),
    );

    match meta.ty() {
        StatType::File | StatType::Dev => print_entry(path.to_str().unwrap(), &meta),
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
                let mut buf = [0; 512];
                let path_len = path.to_bytes().len();
                ensure_or!(
                    path_len + name.len() + 2 <= buf.len(),
                    continue,
                    "path too long"
                );
                buf[..path_len].copy_from_slice(path.to_bytes());
                buf[path_len] = b'/';
                buf[path_len + 1..][..name.len()].copy_from_slice(name.as_bytes());
                buf[path_len + 1 + name.len()] = 0;
                let file_path =
                    CStr::from_bytes_with_nul(&buf[..path_len + 1 + name.len() + 1]).unwrap();
                let meta = try_or!(
                    fs::metadata(file_path),
                    continue,
                    e => "cannot stat {}: {e}", file_path.to_str().unwrap(),
                );
                print_entry(name, &meta);
            }
        }
    }
}

fn main() {
    let args = env::args_cstr();

    if args.len() < 1 {
        ls(c".");
        process::exit(0);
    }
    for arg in args {
        ls(arg);
    }
    process::exit(0);
}
