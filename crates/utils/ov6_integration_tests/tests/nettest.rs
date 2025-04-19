#![cfg(test)]

use std::{fmt::Write as _, path::Path, sync::Arc, time::Duration};

use anyhow::{Context as _, anyhow};
use ov6_integration_tests::{Qemu, Runner, logged_command::LoggedCommand, monitor, runner};

const TIMEOUT: Duration = Duration::from_secs(60);

fn spawn_nettest(
    qemu: &Qemu,
    command: &str,
    runner_id: usize,
    project_root_dir: &Path,
    workspace_dir: &Path,
) -> Result<LoggedCommand, anyhow::Error> {
    let qemu_monitor_sock = qemu.monitor_sock();

    let mut nettest = ov6_integration_tests::project_root_command("cargo", project_root_dir);
    nettest
        .args([
            "run",
            "-p",
            "ov6_net_utilities",
            "--bin",
            "nettest",
            "--",
            command,
            "--server-port",
            "0",
            "--qemu-monitor",
        ])
        .arg(qemu_monitor_sock);

    LoggedCommand::new(nettest, runner_id, "nettest", workspace_dir)
}

async fn wait_server_listen(command: &str, nettest: &LoggedCommand) -> Result<u16, anyhow::Error> {
    let listen_str = format!("{command}: listening for UDP packets");
    let port_str = format!("{command}: server UDP port is ");

    nettest.wait_output(0, |s| s.contains(&listen_str)).await?;

    let stdout = Arc::clone(nettest.stdout());
    let stdout = stdout.lock().unwrap();
    let (_, server_port) = stdout
        .lines()
        .find_map(|line| line.split_once(&port_str))
        .ok_or_else(|| anyhow!("output not found"))?;
    let server_port = server_port.parse().context("invalid port number")?;
    Ok(server_port)
}

async fn test_with_command(
    r: Runner,
    command: &str,
    wait_listen: bool,
    kill_command: bool,
) -> Result<(), anyhow::Error> {
    let runner_id = r.id();
    let project_root_dir = r.project_root();
    let workspace_dir = r.workspace_dir().to_path_buf();

    let (exit_status, stdout, mut nettest) = monitor::run_test(r, TIMEOUT, async |qemu, _gdb| {
        let nettest = spawn_nettest(qemu, command, runner_id, project_root_dir, &workspace_dir)?;
        let mut cmd = format!("nettest -T {command}");
        if wait_listen {
            let server_port = wait_server_listen(command, &nettest)
                .await
                .context("failed to get server_port")?;
            write!(&mut cmd, " -- -p {server_port}")?;
        }
        monitor::run_commands(qemu, 0, [cmd]).await?;
        Ok(nettest)
    })
    .await?;

    assert!(exit_status.success());
    assert!(stdout.contains("PASSED"));
    assert!(!stdout.contains("FAILED"));

    if kill_command {
        nettest.kill().await?;
        let _ = nettest.wait_terminate().await?;
    } else {
        let (exit_status, stdout) = nettest.wait_terminate().await?;
        assert!(exit_status.success());
        assert!(stdout.contains("OK"));
    }

    Ok(())
}

#[cfg_attr(miri, ignore)]
#[tokio::test]
async fn txone() -> Result<(), anyhow::Error> {
    let r = runner!("txone").await?;
    test_with_command(r, "txone", true, false).await?;
    Ok(())
}

#[cfg_attr(miri, ignore)]
#[tokio::test]
async fn rx() -> Result<(), anyhow::Error> {
    let r = runner!("rx").await?;
    test_with_command(r, "rx", false, true).await?;
    Ok(())
}

#[cfg_attr(miri, ignore)]
#[tokio::test]
async fn rx2() -> Result<(), anyhow::Error> {
    let r = runner!("rx2").await?;
    test_with_command(r, "rx2", false, true).await?;
    Ok(())
}

#[cfg_attr(miri, ignore)]
#[tokio::test]
async fn rxburst() -> Result<(), anyhow::Error> {
    let r = runner!("rxburst").await?;
    test_with_command(r, "rxburst", false, true).await?;
    Ok(())
}

#[cfg_attr(miri, ignore)]
#[tokio::test]
async fn tx() -> Result<(), anyhow::Error> {
    let r = runner!("tx").await?;
    test_with_command(r, "tx", true, false).await?;
    Ok(())
}

#[cfg_attr(miri, ignore)]
#[tokio::test]
async fn ping0() -> Result<(), anyhow::Error> {
    let r = runner!("ping0").await?;
    test_with_command(r, "ping0", true, true).await?;
    Ok(())
}

#[cfg_attr(miri, ignore)]
#[tokio::test]
async fn ping1() -> Result<(), anyhow::Error> {
    let r = runner!("ping1").await?;
    test_with_command(r, "ping1", true, true).await?;
    Ok(())
}

#[cfg_attr(miri, ignore)]
#[tokio::test]
async fn ping2() -> Result<(), anyhow::Error> {
    let r = runner!("ping2").await?;
    test_with_command(r, "ping2", true, true).await?;
    Ok(())
}

#[cfg_attr(miri, ignore)]
#[tokio::test]
async fn ping3() -> Result<(), anyhow::Error> {
    let r = runner!("ping3").await?;
    test_with_command(r, "ping3", true, true).await?;
    Ok(())
}

#[cfg_attr(miri, ignore)]
#[tokio::test]
async fn dns() -> Result<(), anyhow::Error> {
    let r = runner!("dns").await?;
    let command = "dns";

    let (exit_status, stdout, ()) = monitor::run_test(r, TIMEOUT, async |qemu, _gdb| {
        monitor::run_commands(qemu, 0, [format!("nettest -T {command}")]).await?;
        Ok(())
    })
    .await?;

    assert!(exit_status.success());
    assert!(stdout.contains("PASSED"));
    assert!(!stdout.contains("FAILED"));

    Ok(())
}
