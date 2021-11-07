use std::process::Command;

fn main() {
    // Expose GIT_SHA env var
    let gitsha = Command::new("git").args(&["rev-parse", "HEAD"]).output();
    if let Ok(sha) = gitsha {
        let sha = String::from_utf8(sha.stdout).unwrap();
        println!("cargo:rustc-env=GIT_SHA={}", sha);
    }
}
