#![no_std]

use core::ffi::CStr;

use xv6_user_lib::{
    env, eprintln,
    fs::{self, Metadata, StatType},
    println, process,
};

fn print_entry(name: &str, meta: &Metadata) {
    let ty = match meta.ty() {
        StatType::Dir => "dir",
        StatType::File => "file",
        StatType::Dev => "dev",
    };
    println!("{:16} {:4} {:6} {:12}", name, ty, meta.ino(), meta.size(),)
}

fn ls(path: &CStr) {
    let prog = env::arg0();

    let Ok(meta) = fs::metadata(path) else {
        eprintln!("{prog}: cannot stat {}", path.to_str().unwrap());
        return;
    };

    match meta.ty() {
        StatType::File | StatType::Dev => print_entry(path.to_str().unwrap(), &meta),
        StatType::Dir => {
            let Ok(entries) = fs::read_dir(path) else {
                eprintln!(
                    "{prog}: cannot open {} as directory",
                    path.to_str().unwrap()
                );
                return;
            };
            for ent in entries {
                let Ok(ent) = ent else {
                    eprintln!("{prog}: cannot read directory entry");
                    return;
                };
                let name = ent.name();
                let mut buf = [0; 512];
                let path_len = path.to_bytes().len();
                if path_len + 1 + name.len() + 1 > buf.len() {
                    eprintln!("{prog}: path too long");
                    continue;
                }
                buf[..path_len].copy_from_slice(path.to_bytes());
                buf[path_len] = b'/';
                buf[path_len + 1..][..name.len()].copy_from_slice(name.as_bytes());
                buf[path_len + 1 + name.len()] = 0;
                let file_path =
                    CStr::from_bytes_with_nul(&buf[..path_len + 1 + name.len() + 1]).unwrap();
                let Ok(meta) = fs::metadata(file_path) else {
                    eprintln!("{prog}: cannot stat {}", file_path.to_str().unwrap());
                    continue;
                };
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
