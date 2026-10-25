//! The fork commit `health.get` reports (TelosWeaver, Spec v1.5 §12.3, PH0-RN-06): `git rev-parse HEAD` of the
//! TelosWeaver repository this was built from, with `-dirty` when this directory has uncommitted changes, or
//! `unknown` outside a Git checkout.

use std::process::Command;

fn git(args: &[&str]) -> Option<String> {
    let out = Command::new("git").args(args).output().ok().filter(|o| o.status.success())?;
    Some(String::from_utf8_lossy(&out.stdout).trim().to_string())
}

fn main() {
    let commit = git(&["rev-parse", "HEAD"]).filter(|c| !c.is_empty());
    let dirty = git(&["status", "--porcelain", "--", "."]).is_some_and(|s| !s.is_empty());
    let value = match commit {
        Some(c) if dirty => format!("{c}-dirty"),
        Some(c) => c,
        None => "unknown".into(),
    };
    println!("cargo:rustc-env=TELOS_RN_FORK_COMMIT={value}");
    // Again when the sources or the checked-out commit change.
    for path in ["src", "Cargo.toml", "Cargo.lock", "build.rs"] {
        println!("cargo:rerun-if-changed={path}");
    }
    for spec in ["HEAD", "index"] {
        if let Some(p) = git(&["rev-parse", "--git-path", spec]) {
            println!("cargo:rerun-if-changed={p}");
        }
    }
    if let Some(r) = git(&["symbolic-ref", "-q", "HEAD"]).and_then(|r| git(&["rev-parse", "--git-path", &r])) {
        println!("cargo:rerun-if-changed={r}");
    }
}
