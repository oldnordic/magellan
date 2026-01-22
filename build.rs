use std::process::Command;

fn main() {
    // Get commit SHA
    let commit_sha = Command::new("git")
        .args(["rev-parse", "--short", "HEAD"])
        .output()
        .ok()
        .filter(|output| output.status.success())
        .and_then(|output| String::from_utf8(output.stdout).ok())
        .map(|s| s.trim().to_string())
        .unwrap_or_else(|| "unknown".to_string());

    // Get build date
    let build_date = Command::new("date")
        .args(["+%Y-%m-%d"])
        .output()
        .ok()
        .filter(|output| output.status.success())
        .and_then(|output| String::from_utf8(output.stdout).ok())
        .map(|s| s.trim().to_string())
        .unwrap_or_else(|| "unknown".to_string());

    // Get rustc version
    let rustc_version = Command::new("rustc")
        .arg("--version")
        .output()
        .ok()
        .filter(|output| output.status.success())
        .and_then(|output| String::from_utf8(output.stdout).ok())
        .map(|s| {
            // Parse "rustc 1.92.0 (..." -> "1.92.0"
            s.trim()
                .strip_prefix("rustc ")
                .and_then(|v| v.split_whitespace().next())
                .unwrap_or("unknown")
                .to_string()
        })
        .unwrap_or_else(|| "unknown".to_string());

    // Set cargo environment variables
    println!("cargo:rustc-env=MAGELLAN_COMMIT_SHA={}", commit_sha);
    println!("cargo:rustc-env=MAGELLAN_BUILD_DATE={}", build_date);
    println!("cargo:rustc-env=MAGELLAN_RUSTC_VERSION={}", rustc_version);

    // Rebuild if git HEAD changes
    println!("cargo:rerun-if-changed=.git/HEAD");
    println!("cargo:rerun-if-env-changed=MAGELLAN_COMMIT_SHA");
}
