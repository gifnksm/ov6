#![cfg(test)]

use std::time::Duration;

use ov6_integration_tests::{monitor, runner};

const TIMEOUT: Duration = Duration::from_secs(30);

#[cfg_attr(miri, ignore)]
#[tokio::test]
async fn sleep_no_arguments() -> Result<(), anyhow::Error> {
    let r = runner!("sleep_no_arguments").await?;
    let (exit_status, stdout) = monitor::run_test(r, TIMEOUT, async |qemu, _gdb| {
        monitor::run_commands(qemu, 0, ["sleep", "halt"]).await?;
        Ok(())
    })
    .await?;
    assert!(exit_status.success());
    assert!(stdout.contains("Usage: sleep"));
    Ok(())
}

#[cfg_attr(miri, ignore)]
#[tokio::test]
async fn sleep_race() -> Result<(), anyhow::Error> {
    let r = runner!("sleep_race").await?;
    let (exit_status, _stdout) = monitor::run_test(r, TIMEOUT, async |qemu, _gdb| {
        monitor::run_commands(
            qemu,
            0,
            ["(sleep 3; abort) &; (sleep 10; abort) &; (sleep 1; halt)"],
        )
        .await?;
        Ok(())
    })
    .await?;
    assert!(exit_status.success());
    Ok(())
}

#[cfg_attr(miri, ignore)]
#[tokio::test]
async fn pingpong() -> Result<(), anyhow::Error> {
    let r = runner!("pingpong").await?;
    let (exit_status, stdout) = monitor::run_test(r, TIMEOUT, async |qemu, _gdb| {
        monitor::run_commands(qemu, 0, ["pingpong", "halt"]).await?;
        Ok(())
    })
    .await?;
    assert!(exit_status.success());
    assert!(stdout.contains("received ping"));
    assert!(stdout.contains("received pong"));
    Ok(())
}
