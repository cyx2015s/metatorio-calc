use std::fs;
use std::path::Path;
use std::process::Command;

fn main() {
    // Get the current Git hash
    let git_hash = Command::new("git")
        .args(["rev-parse", "--short", "HEAD"])
        .output()
        .map(|output| String::from_utf8_lossy(&output.stdout).trim().to_string())
        .unwrap_or_else(|_| "unknown".to_string());

    // Write the Git hash to a file
    let out_dir = std::env::var("OUT_DIR").unwrap();
    let dest_path = Path::new(&out_dir).join("git_hash.rs");
    fs::write(
        &dest_path,
        format!("pub const GIT_HASH: &str = \"{}\";", git_hash),
    )
    .unwrap();

    if std::env::var_os("CARGO_CFG_WINDOWS").is_some() {
        winresource::WindowsResource::new()
            // This path can be absolute, or relative to your crate root.
            .set_icon("assets/icon.ico")
            .compile().unwrap();
    }
}
