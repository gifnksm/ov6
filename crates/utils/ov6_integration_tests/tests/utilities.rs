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
    for p in primes {
        assert!(stdout.contains(&format!("\nprime {p}\n")));
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
    assert!(stdout.contains(&format!("./{file}\n")));
    assert!(!stdout.contains("README"));
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
    assert!(stdout.contains(&format!("{dir}/{file}\n")));
    assert!(!stdout.contains(&format!("./{dir}\n")));
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
    assert!(stdout.contains(&format!("./{}/{}\n", dirs[0], needle)));
    assert!(stdout.contains(&format!("./{}/{}/{}\n", dirs[0], dirs[1], needle)));
    assert!(stdout.contains(&format!("./{}/{}\n", dirs[2], needle)));
    Ok(())
}
