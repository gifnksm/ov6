use std::{env, fs, path::PathBuf};

fn main() {
    let target_arch = env::var("CARGO_CFG_TARGET_ARCH").unwrap();
    if target_arch == "riscv64" {
        let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        let linker_script = manifest_dir.join("user.ld");
        let out_dir = PathBuf::from(env::var("OUT_DIR").unwrap());
        fs::copy(&linker_script, out_dir.join("user.ld")).unwrap();
        println!("cargo:rerun-if-changed={}", linker_script.display());
        println!("cargo::rustc-link-search={}", out_dir.display());
    }
}
