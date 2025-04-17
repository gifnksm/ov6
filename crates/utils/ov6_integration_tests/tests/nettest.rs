#![cfg(test)]

use std::{path::Path, sync::Arc, time::Duration};

use anyhow::{Context as _, anyhow};
use ov6_integration_tests::{logged_command::LoggedCommand, monitor, runner};

const TIMEOUT: Duration = Duration::from_secs(60);

fn spawn_nettest(
    command: &str,
    runner_id: usize,
    project_root_dir: &Path,
    workspace_dir: &Path,
) -> Result<LoggedCommand, anyhow::Error> {
    let mut nettest = ov6_integration_tests::project_root_command("cargo", project_root_dir);
    nettest.args([
        "run",
        "-p",
        "ov6_net_utilities",
        "--bin",
        "nettest",
        "--",
        command,
        "--server-port",
        "0",
    ]);

    LoggedCommand::new(nettest, runner_id, "nettest", workspace_dir)
}

async fn wait_server_port(command: &str, nettest: &LoggedCommand) -> Result<u16, anyhow::Error> {
    let listen_str = format!("{command}: listening on a UDP packet");
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

#[cfg_attr(miri, ignore)]
#[tokio::test]
async fn txone() -> Result<(), anyhow::Error> {
    let r = runner!("txone").await?;
    let command = "txone";

    let runner_id = r.id();
    let project_root_dir = r.project_root();
    let workspace_dir = r.workspace_dir().to_path_buf();

    let (exit_status, stdout, nettest) = monitor::run_test(r, TIMEOUT, async |qemu, _gdb| {
        let nettest = spawn_nettest(command, runner_id, project_root_dir, &workspace_dir)?;
        let server_port = wait_server_port(command, &nettest)
            .await
            .context("failed to get server_port")?;

        monitor::run_commands(
            qemu,
            0,
            [format!("nettest -T {command} -- -p {server_port}")],
        )
        .await?;
        Ok(nettest)
    })
    .await?;

    assert!(exit_status.success());
    assert!(stdout.contains("PASSED"));
    assert!(!stdout.contains("FAILED"));

    let (exit_status, stdout) = nettest.wait_terminate().await?;
    assert!(exit_status.success());
    assert!(stdout.contains("OK"));

    Ok(())
}
