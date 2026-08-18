//! Stamps every binary with the code it was built from.
//!
//! Half of a fault report is knowing which build produced it. Asking the
//! person is unreliable, and answers « I did pull » either way. The
//! commit and its date are read here, at build time, so every log opens
//! with them and the question never has to be asked.

use std::path::{Path, PathBuf};
use std::process::Command;

fn main() {
    let commit = git(&["rev-parse", "--short", "HEAD"]);
    let date = git(&["log", "-1", "--format=%cd", "--date=short"]);

    let build = match (commit, date) {
        (Some(commit), Some(date)) => format!("{commit} {date}"),
        (Some(commit), None) => commit,
        _ => "inconnu".to_string(),
    };
    println!("cargo::rustc-env=ZYR_BUILD={build}");

    watch_the_repository();
}

/// What git answers, or nothing at all when there is no repository.
fn git(arguments: &[&str]) -> Option<String> {
    let output = Command::new("git").args(arguments).output().ok()?;
    if !output.status.success() {
        return None;
    }
    let said = String::from_utf8(output.stdout).ok()?.trim().to_string();
    if said.is_empty() { None } else { Some(said) }
}

/// Asks for this script to be run again when the checked-out code moves.
///
/// Without it the stamp is worked out once and then frozen: a build made
/// after a pull would keep claiming the commit from before, which is
/// exactly the lie this whole file exists to prevent.
fn watch_the_repository() {
    let Some(git_dir) = git(&["rev-parse", "--git-dir"]).map(PathBuf::from) else {
        return;
    };
    // Where the branch points, where the branches are, and where they go
    // once packed away. A path that does not exist is left out: cargo
    // reports it as a fault rather than ignoring it.
    for path in ["HEAD", "refs", "packed-refs"] {
        let watched = git_dir.join(path);
        if watched.exists() {
            println!("cargo::rerun-if-changed={}", shown(&watched));
        }
    }
}

fn shown(path: &Path) -> String {
    path.display().to_string()
}
