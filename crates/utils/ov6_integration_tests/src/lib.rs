use std::path::Path;

use tokio::process::Command;

pub use self::{gdb::Gdb, qemu::Qemu, runner::Runner};

mod gdb;
pub mod helper;
pub mod monitor;
mod qemu;
mod runner;

fn make_command(project_root: &Path) -> Command {
    let mut cmd = Command::new("make");
    cmd.current_dir(project_root);
    cmd
}

#[macro_export]
macro_rules! runner {
    ($name:expr) => {
        $crate::Runner::new(env!("CARGO_PKG_NAME"), module_path!(), $name)
    };
}
