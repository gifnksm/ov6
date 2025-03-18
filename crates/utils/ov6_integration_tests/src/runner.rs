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

const DEFAULT_MAKE_PROFILE: &str = "release";

static RUNNER_ID: AtomicUsize = AtomicUsize::new(0);

pub struct Runner {
    id: usize,
    project_root: &'static Path,
    workspace_dir: PathBuf,
    kernel_path: PathBuf,
    fs_path: PathBuf,
}

impl Runner {
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

    pub async fn launch(self) -> Result<(Qemu, Gdb), anyhow::Error> {
        let gdb_sock = env::temp_dir().join(format!("ov6.{}.{}.socket", process::id(), self.id));

        let qemu = Qemu::new(
            self.id,
            self.project_root,
            &self.workspace_dir,
            &self.kernel_path,
            &self.fs_path,
            &gdb_sock,
        )?;

        let mut gdb = Gdb::connect(gdb_sock).await?;

        gdb.cont().await?;
        monitor::wait_boot(&qemu, 0).await?;

        Ok((qemu, gdb))
    }
}

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
    let fs_src = project_root.join("fs.img");

    let kernel_dst = workspace_dir.join("kernel");
    let fs_dst = workspace_dir.join("fs.img");

    fs::copy(&kernel_src, &kernel_dst).context("copy kernel failed")?;
    fs::copy(&fs_src, &fs_dst).context("copy fs.img failed")?;

    Ok((kernel_dst, fs_dst))
}
