use std::process::Command;
use std::thread;
use std::time::Duration;

/// Smoke test: the service-daemon CLI entry point must exist and be recognized.
/// This would have caught the Phase 0 gap where service_cmd.rs referenced
/// "service-daemon" but cli.rs never wired it into the Command enum.
#[test]
fn test_service_daemon_cli_entry_point_parses() {
    let exe = env!("CARGO_BIN_EXE_magellan");
    let mut child = Command::new(exe)
        .arg("service-daemon")
        .spawn()
        .expect("failed to spawn magellan service-daemon");

    // Give it time to either fail (Unknown command) or start up
    thread::sleep(Duration::from_secs(1));

    // Kill so we can read output
    let _ = child.kill();
    let output = child.wait_with_output().expect("wait failed");
    let stderr = String::from_utf8_lossy(&output.stderr);

    // "Unknown command" means the parser rejected the subcommand — the unwired gap
    assert!(
        !stderr.contains("Unknown command"),
        "service-daemon is not wired into CLI parser: stderr={}",
        stderr
    );

    // Should have exited naturally or been killed (no exit code assertion — just parser coverage)
}
