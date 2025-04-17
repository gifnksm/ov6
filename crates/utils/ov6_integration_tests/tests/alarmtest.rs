#![cfg(test)]

use std::time::Duration;

use ov6_integration_tests::{monitor, runner};

const TIMEOUT: Duration = Duration::from_secs(60);

#[cfg_attr(miri, ignore)]
#[tokio::test]
async fn alarmtest() -> Result<(), anyhow::Error> {
    let r = runner!("alarmtest").await?;
    let (exit_status, stdout, ()) = monitor::run_test(r, TIMEOUT, async |qemu, _gdb| {
        monitor::run_commands(qemu, 0, ["alarmtest -T"]).await?;
        Ok(())
    })
    .await?;
    assert!(exit_status.success());
    assert!(stdout.contains("PASSED"));
    assert!(!stdout.contains("FAILED"));
    Ok(())
}
