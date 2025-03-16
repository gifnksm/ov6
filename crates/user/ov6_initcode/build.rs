use std::env;

fn main() {
    let target_arch = env::var("CARGO_CFG_TARGET_ARCH").unwrap();
    if target_arch == "riscv64" {
        // user.ld is copied by the build script in the ov6_user_lib crate
        println!("cargo::rustc-link-arg=-Tuser.ld");
    }
}
