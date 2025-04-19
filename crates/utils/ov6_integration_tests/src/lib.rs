//! Integration testing utilities for the OV6 project.
//!
//! This crate provides modules and utilities for managing QEMU instances,
//! interacting with GDB, and running integration tests. It includes helper
//! functions, macros, and abstractions to simplify test setup and execution.

use std::path::Path;

use tokio::process::Command;

pub use self::{gdb::Gdb, qemu::Qemu, runner::Runner};

mod gdb;
pub mod helper;
pub mod logged_command;
pub mod monitor;
mod qemu;
mod runner;

/// Creates a `Command` for executing a given command in the project root
/// directory.
///
/// This function sets the current working directory of the command to the
/// specified project root.
#[must_use]
pub fn project_root_command(command: &str, project_root: &Path) -> Command {
    let mut cmd = Command::new(command);
    cmd.current_dir(project_root);
    cmd
}

/// Creates a `Command` for running `make` in the project root directory.
///
/// This function is a convenience wrapper around `project_root_command`
/// specifically for invoking `make`.
#[must_use]
pub fn make_command(project_root: &Path) -> Command {
    project_root_command("make", project_root)
}

/// A macro for creating a new `Runner` instance.
///
/// This macro simplifies the creation of a `Runner` by automatically
/// including the package name and module path.
///
/// # Arguments
///
/// * `$name` - The name of the test or runner.
///
/// # Examples
///
/// ```
/// let runner = runner!("test_name");
/// ```
#[macro_export]
macro_rules! runner {
    ($name:expr) => {
        $crate::Runner::new(env!("CARGO_PKG_NAME"), module_path!(), $name)
    };
}
