//! Stamps the build with the commit it came from, as `KARUKAN_VERSION`
//! (read by `karukan_engine::VERSION`).
//!
//! - `KARUKAN_VERSION` set in the environment wins as is, so a packager
//!   building from a tarball can name the version it packaged.
//! - Otherwise, inside this repository's own checkout, the Cargo version
//!   gets the commit as build metadata: `0.1.0+g1396bb2`.
//! - Anywhere else (a tarball, or a checkout of some other repository that
//!   vendors this crate) it is the Cargo version alone.
//!
//! Uncommitted changes are not marked: this script reruns only when HEAD
//! moves, so a dirty flag would describe the tree at some earlier build.

use std::path::{Path, PathBuf};
use std::process::Command;

fn main() {
    println!("cargo:rerun-if-env-changed=KARUKAN_VERSION");
    let version = match std::env::var("KARUKAN_VERSION") {
        Ok(v) if !v.trim().is_empty() => v.trim().to_string(),
        _ => {
            let pkg = std::env::var("CARGO_PKG_VERSION").expect("set by cargo");
            match commit() {
                Some(sha) => format!("{pkg}+g{sha}"),
                None => pkg,
            }
        }
    };
    println!("cargo:rustc-env=KARUKAN_VERSION={version}");
}

fn git(dir: &Path, args: &[&str]) -> Option<String> {
    let out = Command::new("git")
        .args(args)
        .current_dir(dir)
        .output()
        .ok()?;
    if !out.status.success() {
        return None;
    }
    let s = String::from_utf8(out.stdout).ok()?.trim().to_string();
    (!s.is_empty()).then_some(s)
}

/// The abbreviated HEAD commit, if this crate sits in the workspace of the
/// checkout `git` finds. Registers the files that move with HEAD so the
/// stamp follows commits and branch switches.
fn commit() -> Option<String> {
    let manifest = PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").ok()?);
    let workspace = manifest.parent()?.canonicalize().ok()?;
    let toplevel = PathBuf::from(git(&manifest, &["rev-parse", "--show-toplevel"])?);
    if toplevel.canonicalize().ok()? != workspace {
        return None;
    }
    let mut watched = vec!["HEAD".to_string(), "packed-refs".to_string()];
    if let Some(head_ref) = git(&manifest, &["symbolic-ref", "-q", "HEAD"]) {
        watched.push(head_ref);
    }
    for path in watched {
        if let Some(p) = git(&manifest, &["rev-parse", "--git-path", &path]) {
            let p = manifest.join(p);
            if p.exists() {
                println!("cargo:rerun-if-changed={}", p.display());
            }
        }
    }
    git(&manifest, &["rev-parse", "--short=7", "HEAD"])
}
