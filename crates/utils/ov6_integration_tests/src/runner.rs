//! Test runner for integration tests.
//!
//! This module provides the `Runner` struct, which manages the setup and
//! execution of integration tests. It handles workspace preparation, QEMU
//! and GDB initialization, and test lifecycle management.

use std::{
    env,
    fs::{self, File},
    path::{Path, PathBuf},
    process,
    sync::atomic::{AtomicUsize, Ordering},
};

use anyhow::{Context as _, ensure};
use fs4::fs_std::FileExt as _;
use tokio::task;

use crate::{Gdb, Qemu, monitor};

/// The default build profile used for the test runner.
const DEFAULT_MAKE_PROFILE: &str = "release";

/// A global atomic counter for assigning unique runner IDs.
static RUNNER_ID: AtomicUsize = AtomicUsize::new(0);

/// Represents a test runner for integration tests.
///
/// The `Runner` struct manages the workspace, QEMU instance, and GDB
/// connection required for running integration tests.
pub struct Runner {
    /// The unique ID of the runner.
    id: usize,
    /// The root directory of the project.
    project_root: &'static Path,
    /// The workspace directory for the test.
    workspace_dir: PathBuf,
    /// The path to the kernel binary.
    kernel_path: PathBuf,
    /// The path to the filesystem image.
    fs_path: PathBuf,
}

impl Runner {
    /// Creates a new `Runner` instance.
    ///
    /// This function initializes the test workspace and prepares the kernel
    /// and filesystem artifacts for testing.
    ///
    /// # Errors
    ///
    /// Returns an error if the workspace setup fails or if required artifacts
    /// cannot be prepared.
    #[expect(clippy::missing_panics_doc)]
    pub async fn new(
        pkg_name: &str,
        module_path: &str,
        fn_name: &str,
    ) -> Result<Self, anyhow::Error> {
        let id = RUNNER_ID.fetch_add(1, Ordering::Relaxed);
        let manifest_dir = Path::new(env!("CARGO_MANIFEST_DIR"));
        let project_root = manifest_dir.ancestors().nth(3).unwrap();

        let mut workspace_dir = project_root
            .join("target")
            .join("ov6")
            .join(DEFAULT_MAKE_PROFILE)
            .join(pkg_name);

        for component in module_path.split("::") {
            workspace_dir.push(component);
        }
        workspace_dir.push(fn_name);

        let (kernel_path, fs_path) = task::spawn_blocking({
            let project_root = project_root.to_owned();
            let workspace_dir = workspace_dir.clone();
            move || setup_workspace(&project_root, &workspace_dir)
        })
        .await??;

        Ok(Self {
            id,
            project_root,
            workspace_dir,
            kernel_path,
            fs_path,
        })
    }

    /// Returns the unique ID of the runner.
    #[must_use]
    pub fn id(&self) -> usize {
        self.id
    }

    /// Returns the root directory of the project.
    #[must_use]
    pub fn project_root(&self) -> &'static Path {
        self.project_root
    }

    /// Returns the workspace directory for the test.
    #[must_use]
    pub fn workspace_dir(&self) -> &Path {
        &self.workspace_dir
    }

    /// Launches the QEMU and GDB instances for the test.
    ///
    /// This function initializes the QEMU and GDB instances, waits for the
    /// kernel to boot, and prepares the test environment.
    ///
    /// # Errors
    ///
    /// Returns an error if the QEMU or GDB instances fail to initialize or if
    /// the kernel boot process fails.
    pub async fn launch(self) -> Result<(Qemu, Gdb), anyhow::Error> {
        let temp_dir = env::temp_dir();
        let gdb_sock = temp_dir.join(format!("ov6.{}.{}.gdb.socket", process::id(), self.id));
        let qemu_monitor_sock = temp_dir.join(format!(
            "ov6.{}.{}.qemu-monitor.socket",
            process::id(),
            self.id
        ));

        let qemu = Qemu::new(
            self.id,
            self.project_root,
            &self.workspace_dir,
            &self.kernel_path,
            &self.fs_path,
            &gdb_sock,
            &qemu_monitor_sock,
        )?;

        let mut gdb = Gdb::connect(gdb_sock).await?;

        gdb.cont().await?;
        monitor::wait_boot(&qemu, 0).await?;

        Ok((qemu, gdb))
    }
}

/// Sets up the test workspace.
///
/// This function prepares the workspace directory, locks the build process,
/// and copies the kernel and filesystem artifacts to the workspace.
///
/// # Errors
///
/// Returns an error if the workspace setup fails or if the artifacts cannot
/// be copied.
fn setup_workspace(
    project_root: &Path,
    workspace_dir: &Path,
) -> Result<(PathBuf, PathBuf), anyhow::Error> {
    fs::create_dir_all(workspace_dir).context("create workspace failed")?;

    let lockfile_path = project_root
        .join("target")
        .join("ov6")
        .join("test_runner.lock");

    let lockfile = File::create(lockfile_path).context("open lockfile failed")?;
    lockfile.lock_exclusive().context("lock lockfile failed")?;

    let make_status = crate::make_command(project_root)
        .into_std()
        .args(["all"])
        .status()
        .context("make all execute failed")?;
    ensure!(make_status.success(), "make all failed");

    let artifacts_dir = project_root
        .join("target")
        .join("ov6")
        .join(DEFAULT_MAKE_PROFILE);

    let kernel_src = artifacts_dir.join("kernel");
    let fs_src = artifacts_dir.join("fs.img");

    let kernel_dst = workspace_dir.join("kernel");
    let fs_dst = workspace_dir.join("fs.img");

    let _ = fs::remove_file(&kernel_dst);
    let _ = fs::remove_file(&fs_dst);
    fs::copy(&kernel_src, &kernel_dst).context("copy kernel failed")?;
    fs::copy(&fs_src, &fs_dst).context("copy fs.img failed")?;

    Ok((kernel_dst, fs_dst))
}
