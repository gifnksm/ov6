use std::path::Path;

use tokio::process::Command;

pub use self::{gdb::Gdb, qemu::Qemu, runner::Runner};

mod gdb;
pub mod helper;
pub mod logged_command;
pub mod monitor;
mod qemu;
mod runner;

pub fn project_root_command(command: &str, project_root: &Path) -> Command {
    let mut cmd = Command::new(command);
    cmd.current_dir(project_root);
    cmd
}

pub fn make_command(project_root: &Path) -> Command {
    project_root_command("make", project_root)
}

#[macro_export]
macro_rules! runner {
    ($name:expr) => {
        $crate::Runner::new(env!("CARGO_PKG_NAME"), module_path!(), $name)
    };
}
