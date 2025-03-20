#![cfg(test)]

use std::time::Duration;

use ov6_integration_tests::{helper, monitor, runner};

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
    let lines = stdout.lines().collect::<Vec<_>>();
    assert!(lines.iter().any(|s| s.starts_with("Usage: sleep")));
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
    let lines = stdout.lines().collect::<Vec<_>>();
    assert!(lines.iter().any(|s| s.ends_with("received ping")));
    assert!(lines.iter().any(|s| s.ends_with(&"received pong")));
    Ok(())
}

#[cfg_attr(miri, ignore)]
#[tokio::test]
async fn primes() -> Result<(), anyhow::Error> {
    let r = runner!("primes").await?;
    let (exit_status, stdout) = monitor::run_test(r, TIMEOUT, async |qemu, _gdb| {
        monitor::run_commands(qemu, 0, ["primes", "halt"]).await?;
        Ok(())
    })
    .await?;
    assert!(exit_status.success());
    let primes = [
        2, 3, 5, 7, 11, 13, 17, 19, 23, 29, 31, 37, 41, 43, 47, 53, 59, 61, 67, 71, 73, 79, 83, 89,
        97, 101, 103, 107, 109, 113, 127, 131, 137, 139, 149, 151, 157, 163, 167, 173, 179, 181,
        191, 193, 197, 199, 211, 223, 227, 229, 233, 239, 241, 251, 257, 263, 269,
    ];
    let lines = stdout.lines().collect::<Vec<_>>();
    for p in primes {
        assert!(lines.contains(&format!("prime {p}").as_str()));
    }
    Ok(())
}

#[cfg_attr(miri, ignore)]
#[tokio::test]
async fn find_current_dir() -> Result<(), anyhow::Error> {
    let r = runner!("find_current_dir").await?;
    let file = helper::random_str(8);
    let (exit_status, stdout) = monitor::run_test(r, TIMEOUT, async |qemu, _gdb| {
        monitor::run_commands(
            qemu,
            0,
            [&format!("echo > {file}"), &format!("find . {file}"), "halt"],
        )
        .await?;
        Ok(())
    })
    .await?;
    assert!(exit_status.success());
    let lines = stdout.lines().collect::<Vec<_>>();
    assert!(lines.contains(&format!("./{file}").as_str()));
    assert!(!lines.contains(&"README"));
    Ok(())
}

#[cfg_attr(miri, ignore)]
#[tokio::test]
async fn find_subdir() -> Result<(), anyhow::Error> {
    let r = runner!("find_subdir").await?;
    let dir = helper::random_str(8);
    let file = helper::random_str(8);
    let (exit_status, stdout) = monitor::run_test(r, TIMEOUT, async |qemu, _gdb| {
        monitor::run_commands(
            qemu,
            0,
            [
                &format!("echo > {file}"),
                &format!("mkdir {dir}"),
                &format!("echo > {dir}/{file}"),
                &format!("find {dir} {file}"),
                "halt",
            ],
        )
        .await?;
        Ok(())
    })
    .await?;
    assert!(exit_status.success());
    let lines = stdout.lines().collect::<Vec<_>>();
    assert!(lines.contains(&format!("{dir}/{file}").as_str()));
    assert!(!lines.contains(&format!("./{dir}").as_str()));
    Ok(())
}

#[cfg_attr(miri, ignore)]
#[tokio::test]
async fn find_recursive() -> Result<(), anyhow::Error> {
    let r = runner!("find_recursive").await?;
    let needle = helper::random_str(8);
    let dirs = [
        helper::random_str(8),
        helper::random_str(8),
        helper::random_str(8),
    ];
    let (exit_status, stdout) = monitor::run_test(r, TIMEOUT, async |qemu, _gdb| {
        monitor::run_commands(
            qemu,
            0,
            [
                &format!("mkdir {}", dirs[0]),
                &format!("echo > {}/{}", dirs[0], needle),
                &format!("mkdir {}/{}", dirs[0], dirs[1]),
                &format!("echo > {}/{}/{}", dirs[0], dirs[1], needle),
                &format!("mkdir {}", dirs[2]),
                &format!("echo > {}/{}", dirs[2], needle),
                &format!("find . {needle}"),
                "halt",
            ],
        )
        .await?;
        Ok(())
    })
    .await?;
    assert!(exit_status.success());
    let lines = stdout.lines().collect::<Vec<_>>();
    assert!(lines.contains(&format!("./{}/{}", dirs[0], needle).as_str()));
    assert!(lines.contains(&format!("./{}/{}/{}", dirs[0], dirs[1], needle).as_str()));
    assert!(lines.contains(&format!("./{}/{}", dirs[2], needle).as_str()));
    Ok(())
}

#[cfg_attr(miri, ignore)]
#[tokio::test]
async fn xargs() -> Result<(), anyhow::Error> {
    let r = runner!("xargs").await?;
    let (exit_status, stdout) = monitor::run_test(r, TIMEOUT, async |qemu, _gdb| {
        monitor::run_commands(
            qemu,
            0,
            [
                "mkdir a",
                "echo hello > a/b",
                "mkdir c",
                "echo hello > c/b",
                "echo hello > b",
                "find . b | xargs grep hello",
                "halt",
            ],
        )
        .await?;
        Ok(())
    })
    .await?;
    assert!(exit_status.success());
    let n = stdout.lines().filter(|s| *s == "hello").count();
    assert_eq!(n, 3);
    Ok(())
}

#[cfg_attr(miri, ignore)]
#[tokio::test]
async fn xargs_multi_line_echo() -> Result<(), anyhow::Error> {
    let r = runner!("xargs_multi_line_echo").await?;
    let (exit_status, stdout) = monitor::run_test(r, TIMEOUT, async |qemu, _gdb| {
        monitor::run_commands(qemu, 0, ["(echo 1 ; echo 2) | xargs -n 1 echo", "halt"]).await?;
        Ok(())
    })
    .await?;
    assert!(exit_status.success());
    let lines = stdout.lines().collect::<Vec<_>>();
    assert!(lines.contains(&"1"));
    assert!(lines.contains(&"2"));
    Ok(())
}
