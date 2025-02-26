use std::{env, path::PathBuf};

fn main() {
    let target_arch = env::var("CARGO_CFG_TARGET_ARCH").unwrap();
    if target_arch == "riscv64" {
        let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        let linker_script = manifest_dir.join("user.ld");
        println!("cargo:rerun-if-changed={}", linker_script.display());
        println!("cargo::rustc-link-arg=-T{}", linker_script.display());
    }
}
