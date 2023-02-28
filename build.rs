/*
 * Copyright Redis Ltd. 2016 - present
 * Licensed under your choice of the Redis Source Available License 2.0 (RSALv2) or
 * the Server Side Public License v1 (SSPLv1).
 */

use std::process::Command;

fn main() {
    // Expose GIT_SHA env var
    let git_sha = Command::new("git")
        .args(["rev-parse", "--short", "HEAD"])
        .output();
    if let Ok(sha) = git_sha {
        let sha = String::from_utf8(sha.stdout).unwrap();
        println!("cargo:rustc-env=GIT_SHA={sha}");
    }
    // Expose GIT_BRANCH env var
    let git_branch = Command::new("git")
        .args(["rev-parse", "--abbrev-ref", "HEAD"])
        .output();
    if let Ok(branch) = git_branch {
        let branch = String::from_utf8(branch.stdout).unwrap();
        println!("cargo:rustc-env=GIT_BRANCH={branch}");
    }
}
