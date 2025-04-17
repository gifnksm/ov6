#![cfg(test)]

use std::time::Duration;

use ov6_integration_tests::{monitor, runner};

const TIMEOUT: Duration = Duration::from_secs(5);

#[cfg_attr(miri, ignore)]
#[tokio::test]
async fn halt() -> Result<(), anyhow::Error> {
    let r = runner!("halt").await?;
    let (exit_status, stdout, ()) = monitor::run_test(r, TIMEOUT, async |qemu, _gdb| {
        monitor::run_commands(qemu, 0, ["halt"]).await?;
        Ok(())
    })
    .await?;
    assert!(exit_status.success());
    assert!(stdout.contains("halt requested"));
    Ok(())
}

#[cfg_attr(miri, ignore)]
#[tokio::test]
async fn abort() -> Result<(), anyhow::Error> {
    let r = runner!("abort").await?;
    let (exit_status, stdout, ()) = monitor::run_test(r, TIMEOUT, async |qemu, _gdb| {
        monitor::run_commands(qemu, 0, ["abort"]).await?;
        Ok(())
    })
    .await?;
    assert_eq!(exit_status.code(), Some(2)); // make exit status
    assert!(stdout.contains("abort requested"));
    Ok(())
}

#[cfg_attr(miri, ignore)]
#[tokio::test]
async fn reboot() -> Result<(), anyhow::Error> {
    let r = runner!("reboot").await?;
    let (exit_status, stdout, ()) = monitor::run_test(r, TIMEOUT, async |qemu, _gdb| {
        let before_reboot = monitor::run_commands(qemu, 0, ["reboot"]).await?;
        monitor::wait_boot(qemu, before_reboot).await?;
        monitor::run_commands(qemu, before_reboot, ["halt"]).await?;
        Ok(())
    })
    .await?;
    assert!(exit_status.success());
    assert!(stdout.contains("reboot requested"));
    Ok(())
}
