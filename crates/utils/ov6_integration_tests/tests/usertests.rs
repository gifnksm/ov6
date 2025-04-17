#![cfg(test)]

use std::time::Duration;

use ov6_integration_tests::{monitor, runner};

const TIMEOUT: Duration = Duration::from_secs(60);

#[cfg_attr(miri, ignore)]
#[tokio::test]
async fn memory() -> Result<(), anyhow::Error> {
    let r = runner!("memory").await?;
    let (exit_status, stdout, ()) = monitor::run_test(r, TIMEOUT, async |qemu, _gdb| {
        monitor::run_commands(qemu, 0, ["usertests -T -t memory"]).await?;
        Ok(())
    })
    .await?;
    assert!(exit_status.success());
    assert!(stdout.contains("PASSED"));
    assert!(!stdout.contains("FAILED"));
    Ok(())
}

#[cfg_attr(miri, ignore)]
#[tokio::test]
async fn simple_fs() -> Result<(), anyhow::Error> {
    let r = runner!("simple_fs").await?;
    let (exit_status, stdout, ()) = monitor::run_test(r, TIMEOUT, async |qemu, _gdb| {
        monitor::run_commands(qemu, 0, ["usertests -T -t simple_fs"]).await?;
        Ok(())
    })
    .await?;
    assert!(exit_status.success());
    assert!(stdout.contains("PASSED"));
    assert!(!stdout.contains("FAILED"));
    Ok(())
}

#[cfg_attr(miri, ignore)]
#[tokio::test]
async fn simple_fork() -> Result<(), anyhow::Error> {
    let r = runner!("simple_fork").await?;
    let (exit_status, stdout, ()) = monitor::run_test(r, TIMEOUT, async |qemu, _gdb| {
        monitor::run_commands(qemu, 0, ["usertests -T -t simple_fork"]).await?;
        Ok(())
    })
    .await?;
    assert!(exit_status.success());
    assert!(stdout.contains("PASSED"));
    assert!(!stdout.contains("FAILED"));
    Ok(())
}

#[cfg_attr(miri, ignore)]
#[tokio::test]
async fn more_fs() -> Result<(), anyhow::Error> {
    let r = runner!("more_fs").await?;
    let (exit_status, stdout, ()) = monitor::run_test(r, TIMEOUT, async |qemu, _gdb| {
        monitor::run_commands(qemu, 0, ["usertests -T -t more_fs"]).await?;
        Ok(())
    })
    .await?;
    assert!(exit_status.success());
    assert!(stdout.contains("PASSED"));
    assert!(!stdout.contains("FAILED"));
    Ok(())
}

#[cfg_attr(miri, ignore)]
#[tokio::test]
async fn more_fork() -> Result<(), anyhow::Error> {
    let r = runner!("more_fork").await?;
    let (exit_status, stdout, ()) = monitor::run_test(r, TIMEOUT, async |qemu, _gdb| {
        monitor::run_commands(qemu, 0, ["usertests -T -t more_fork"]).await?;
        Ok(())
    })
    .await?;
    assert!(exit_status.success());
    assert!(stdout.contains("PASSED"));
    assert!(!stdout.contains("FAILED"));
    Ok(())
}

#[cfg_attr(miri, ignore)]
#[tokio::test]
async fn misc() -> Result<(), anyhow::Error> {
    let r = runner!("misc").await?;
    let (exit_status, stdout, ()) = monitor::run_test(r, TIMEOUT, async |qemu, _gdb| {
        monitor::run_commands(qemu, 0, ["usertests -T -t misc"]).await?;
        Ok(())
    })
    .await?;
    assert!(exit_status.success());
    assert!(stdout.contains("PASSED"));
    assert!(!stdout.contains("FAILED"));
    Ok(())
}

mod slow_fs {
    use super::*;

    #[cfg_attr(miri, ignore)]
    #[tokio::test]
    async fn big_dir() -> Result<(), anyhow::Error> {
        let r = runner!("big_dir").await?;
        let (exit_status, stdout, ()) = monitor::run_test(r, TIMEOUT, async |qemu, _gdb| {
            monitor::run_commands(qemu, 0, ["usertests -T slow_fs::big_dir"]).await?;
            Ok(())
        })
        .await?;
        assert!(exit_status.success());
        assert!(stdout.contains("PASSED"));
        assert!(!stdout.contains("FAILED"));
        Ok(())
    }

    #[cfg_attr(miri, ignore)]
    #[tokio::test]
    async fn many_writes() -> Result<(), anyhow::Error> {
        let r = runner!("many_writes").await?;
        let (exit_status, stdout, ()) = monitor::run_test(r, TIMEOUT, async |qemu, _gdb| {
            monitor::run_commands(qemu, 0, ["usertests -T slow_fs::many_writes"]).await?;
            Ok(())
        })
        .await?;
        assert!(exit_status.success());
        assert!(stdout.contains("PASSED"));
        assert!(!stdout.contains("FAILED"));
        Ok(())
    }

    #[cfg_attr(miri, ignore)]
    #[tokio::test]
    async fn bad_write() -> Result<(), anyhow::Error> {
        let r = runner!("bad_write").await?;
        let (exit_status, stdout, ()) = monitor::run_test(r, TIMEOUT, async |qemu, _gdb| {
            monitor::run_commands(qemu, 0, ["usertests -T slow_fs::bad_write"]).await?;
            Ok(())
        })
        .await?;
        assert!(exit_status.success());
        assert!(stdout.contains("PASSED"));
        assert!(!stdout.contains("FAILED"));
        Ok(())
    }

    #[cfg_attr(miri, ignore)]
    #[tokio::test]
    async fn diskf_full() -> Result<(), anyhow::Error> {
        let r = runner!("disk_full").await?;
        let (exit_status, stdout, ()) = monitor::run_test(r, TIMEOUT, async |qemu, _gdb| {
            monitor::run_commands(qemu, 0, ["usertests -T slow_fs::disk_full"]).await?;
            Ok(())
        })
        .await?;
        assert!(exit_status.success());
        assert!(stdout.contains("PASSED"));
        assert!(!stdout.contains("FAILED"));
        Ok(())
    }

    #[cfg_attr(miri, ignore)]
    #[tokio::test]
    async fn out_of_inodes() -> Result<(), anyhow::Error> {
        let r = runner!("out_of_inodes").await?;
        let (exit_status, stdout, ()) = monitor::run_test(r, TIMEOUT, async |qemu, _gdb| {
            monitor::run_commands(qemu, 0, ["usertests -T slow_fs::out_of_inodes"]).await?;
            Ok(())
        })
        .await?;
        assert!(exit_status.success());
        assert!(stdout.contains("PASSED"));
        assert!(!stdout.contains("FAILED"));
        Ok(())
    }
}

mod slow_proc {
    use super::*;

    #[cfg_attr(miri, ignore)]
    #[tokio::test]
    async fn execout() -> Result<(), anyhow::Error> {
        let r = runner!("execout").await?;
        let (exit_status, stdout, ()) = monitor::run_test(r, TIMEOUT, async |qemu, _gdb| {
            monitor::run_commands(qemu, 0, ["usertests -T slow_proc::execout"]).await?;
            Ok(())
        })
        .await?;
        assert!(exit_status.success());
        assert!(stdout.contains("PASSED"));
        assert!(!stdout.contains("FAILED"));
        Ok(())
    }
}
