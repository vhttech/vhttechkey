use std::path::PathBuf;
use std::process::ExitCode;

use vi_testing::fixtures::all_fixtures;
use vi_testing::replay::{assert_replay, replay_session};

fn main() -> ExitCode {
    tracing_subscriber::fmt::init();

    let args: Vec<String> = std::env::args().collect();
    match args.get(1).map(String::as_str) {
        Some("replay") => cmd_replay(&args[2..]),
        Some("fixtures") => cmd_fixtures(),
        Some("sandbox-status") => cmd_sandbox_status(),
        Some("help") | None => {
            eprintln!("vi-tools — Vietnamese IME diagnostic toolbox");
            eprintln!();
            eprintln!("USAGE:");
            eprintln!("  vi-tools replay <session.vi-replay>...");
            eprintln!("  vi-tools fixtures");
            eprintln!("  vi-tools sandbox-status");
            ExitCode::SUCCESS
        }
        Some(other) => {
            eprintln!("unknown subcommand: {other}");
            ExitCode::FAILURE
        }
    }
}

/// Replay one or more `.vi-replay` files and report pass/fail.
fn cmd_replay(paths: &[String]) -> ExitCode {
    if paths.is_empty() {
        eprintln!("replay: no files specified");
        return ExitCode::FAILURE;
    }

    let mut failures = 0usize;
    for path in paths {
        let p = PathBuf::from(path);
        let bytes = match std::fs::read(&p) {
            Ok(b) => b,
            Err(e) => {
                eprintln!("ERROR  {path}: {e}");
                failures += 1;
                continue;
            }
        };
        let session = match vi_testing::replay::ReplaySession::from_bytes(&bytes) {
            Ok(s) => s,
            Err(e) => {
                eprintln!("ERROR  {path}: failed to deserialize: {e}");
                failures += 1;
                continue;
            }
        };
        let actual = replay_session(&session);
        if actual == session.expected_commits {
            println!("PASS   {}", session.name);
        } else {
            eprintln!(
                "FAIL   {}: expected {:?}, got {:?}",
                session.name, session.expected_commits, actual
            );
            failures += 1;
        }
    }

    if failures == 0 {
        ExitCode::SUCCESS
    } else {
        ExitCode::FAILURE
    }
}

/// Print a table of detected sandboxed applications and their IME compatibility.
fn cmd_sandbox_status() -> ExitCode {
    use vi_daemon::detect::{detect_sandboxed_apps, SandboxedApp};

    let apps = detect_sandboxed_apps();
    if apps.is_empty() {
        println!("No sandboxed applications detected.");
        return ExitCode::SUCCESS;
    }
    println!("{:<8}  {:<12}  IME STATUS", "PID", "TYPE");
    println!("{}", "-".repeat(55));
    for app in &apps {
        match app {
            SandboxedApp::Electron(pid) => {
                println!(
                    "{:<8}  {:<12}  needs-flags (--ozone-platform=wayland)",
                    pid, "electron"
                );
            }
            SandboxedApp::Flatpak { pid, app_id } => {
                println!("{:<8}  {:<12}  OK (portal) [{}]", pid, "flatpak", app_id);
            }
            SandboxedApp::Snap { pid, snap_name } => {
                println!("{:<8}  {:<12}  unsupported [{}]", pid, "snap", snap_name);
            }
        }
    }
    ExitCode::SUCCESS
}

/// Print a summary of all built-in fixture sessions.
fn cmd_fixtures() -> ExitCode {
    let fixtures = all_fixtures();
    println!("Built-in fixture sessions: {}", fixtures.len());
    for f in &fixtures {
        let result = replay_session(f);
        let status = if result == f.expected_commits { "PASS" } else { "FAIL" };
        println!("  [{status}] {} ({:?})", f.name, f.method);
    }
    // Check that fixtures round-trip through assert_replay.
    let mut ok = true;
    for f in &fixtures {
        if std::panic::catch_unwind(|| assert_replay(f)).is_err() {
            eprintln!("fixture '{}' failed assertion", f.name);
            ok = false;
        }
    }
    if ok { ExitCode::SUCCESS } else { ExitCode::FAILURE }
}
