#![cfg(test)]

use std::time::Duration;

use ov6_integration_tests::{monitor, runner};
use regex::Regex;

const TIMEOUT: Duration = Duration::from_secs(30);

#[cfg_attr(miri, ignore)]
#[tokio::test]
async fn trace_read_grep() -> Result<(), anyhow::Error> {
    let r = runner!("trace_read_grep").await?;
    let (exit_status, stdout, ()) = monitor::run_test(r, TIMEOUT, async |qemu, _gdb| {
        monitor::run_commands(qemu, 0, ["trace read grep hello README", "halt"]).await?;
        Ok(())
    })
    .await?;
    assert!(exit_status.success());
    let lines = stdout.lines().collect::<Vec<_>>();
    assert!(lines.iter().any(|s| {
        Regex::new(r"^grep\(\d+\): syscall read \(.*\) -> Ok\(\d+\)")
            .unwrap()
            .is_match(s)
    }));
    assert!(lines.iter().any(|s| {
        Regex::new(r"^grep\(\d+\): syscall read \(.*\) -> Ok\(0\)$")
            .unwrap()
            .is_match(s)
    }));
    Ok(())
}

#[cfg_attr(miri, ignore)]
#[tokio::test]
async fn trace_close_grep() -> Result<(), anyhow::Error> {
    let r = runner!("trace_close_grep").await?;
    let (exit_status, stdout, ()) = monitor::run_test(r, TIMEOUT, async |qemu, _gdb| {
        monitor::run_commands(qemu, 0, ["trace close grep hello README", "halt"]).await?;
        Ok(())
    })
    .await?;
    assert!(exit_status.success());
    let lines = stdout.lines().collect::<Vec<_>>();
    assert!(lines.iter().any(|s| {
        Regex::new(r"^grep\(\d+\): syscall close \(.*\) -> Ok\(\(\)\)$")
            .unwrap()
            .is_match(s)
    }));
    Ok(())
}

#[cfg_attr(miri, ignore)]
#[tokio::test]
async fn trace_exec_open_grep() -> Result<(), anyhow::Error> {
    let r = runner!("trace_exec_open_grep").await?;
    let (exit_status, stdout, ()) = monitor::run_test(r, TIMEOUT, async |qemu, _gdb| {
        monitor::run_commands(qemu, 0, ["trace exec,open grep hello README", "halt"]).await?;
        Ok(())
    })
    .await?;
    assert!(exit_status.success());
    let lines = stdout.lines().collect::<Vec<_>>();
    assert!(lines.iter().any(|s| {
        Regex::new(r"^grep\(\d+\): syscall exec \(.*\) -> Ok\(\(3, .*\)\)$")
            .unwrap()
            .is_match(s)
    }));
    assert!(lines.iter().any(|s| {
        Regex::new(r"^grep\(\d+\): syscall open \(.*\) -> Ok\(RawFd\(3\)\)$")
            .unwrap()
            .is_match(s)
    }));
    Ok(())
}

#[cfg_attr(miri, ignore)]
#[tokio::test]
async fn trace_all_grep() -> Result<(), anyhow::Error> {
    let r = runner!("trace_all_grep").await?;
    let (exit_status, stdout, ()) = monitor::run_test(r, TIMEOUT, async |qemu, _gdb| {
        monitor::run_commands(qemu, 0, ["trace all grep hello README", "halt"]).await?;
        Ok(())
    })
    .await?;
    assert!(exit_status.success());
    let lines = stdout.lines().collect::<Vec<_>>();
    assert!(lines.iter().any(|s| {
        Regex::new(r"^trace\(\d+\): syscall trace \(.*\) -> \(\)$")
            .unwrap()
            .is_match(s)
    }));
    assert!(lines.iter().any(|s| {
        Regex::new(r"^grep\(\d+\): syscall exec \(.*\) -> Ok\(\(3, .*\)\)$")
            .unwrap()
            .is_match(s)
    }));
    assert!(lines.iter().any(|s| {
        Regex::new(r"^grep\(\d+\): syscall open \(.*\) -> Ok\(RawFd\(3\)\)$")
            .unwrap()
            .is_match(s)
    }));
    assert!(lines.iter().any(|s| {
        Regex::new(r"^grep\(\d+\): syscall read \(.*\) -> Ok\(\d+\)")
            .unwrap()
            .is_match(s)
    }));
    assert!(lines.iter().any(|s| {
        Regex::new(r"^grep\(\d+\): syscall read \(.*\) -> Ok\(0\)$")
            .unwrap()
            .is_match(s)
    }));
    assert!(lines.iter().any(|s| {
        Regex::new(r"^grep\(\d+\): syscall close \(.*\) -> Ok\(\(\)\)$")
            .unwrap()
            .is_match(s)
    }));
    Ok(())
}

#[cfg_attr(miri, ignore)]
#[tokio::test]
async fn not_trace() -> Result<(), anyhow::Error> {
    let r = runner!("not_trace").await?;
    let (exit_status, stdout, ()) = monitor::run_test(r, TIMEOUT, async |qemu, _gdb| {
        monitor::run_commands(qemu, 0, ["grep hello README", "halt"]).await?;
        Ok(())
    })
    .await?;
    assert!(exit_status.success());
    let lines = stdout.lines().collect::<Vec<_>>();
    assert!(
        lines
            .iter()
            .any(|s| !Regex::new("syscall").unwrap().is_match(s))
    );
    Ok(())
}

#[cfg_attr(miri, ignore)]
#[tokio::test]
async fn trace_children() -> Result<(), anyhow::Error> {
    let r = runner!("trace_children").await?;
    let (exit_status, stdout, ()) = monitor::run_test(r, TIMEOUT, async |qemu, _gdb| {
        monitor::run_commands(
            qemu,
            0,
            ["trace fork usertests simple_fork::fork_fork_fork", "halt"],
        )
        .await?;
        Ok(())
    })
    .await?;
    assert!(exit_status.success());
    let lines = stdout.lines().collect::<Vec<_>>();
    assert!(lines.iter().any(|s| {
        Regex::new(r"usertests\(3\): syscall fork \(\) -> Ok\(Some\(ProcId\(4\)\)\)$")
            .unwrap()
            .is_match(s)
    }));
    assert!(lines.iter().any(|s| {
        Regex::new(r"usertests\(\d+\): syscall fork \(\) -> Err\(ResourceTempolaryUnavailable\)$")
            .unwrap()
            .is_match(s)
    }));
    Ok(())
}
