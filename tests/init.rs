use assert_cmd::prelude::*;
use common::make_dir;
use predicates::prelude::predicate;
use std::process::Command;

use crate::common::file_contents;

mod common;

#[test]
fn init() -> Result<(), Box<dyn std::error::Error>> {
    let dir = make_dir();
    let mut cmd = Command::cargo_bin("git-rs")?;

    cmd.current_dir(dir.path()).arg("init");
    cmd.assert().success();

    assert!(
        std::fs::metadata(dir.subpath(".git"))?.is_dir(),
        ".git must be a directory"
    );
    assert!(
        std::fs::metadata(dir.subpath(".git/objects"))?.is_dir(),
        ".git/objects must be a directory"
    );
    assert!(
        std::fs::metadata(dir.subpath(".git/refs"))?.is_dir(),
        ".git/refs must be a directory"
    );
    assert!(
        std::fs::metadata(dir.subpath(".git/HEAD"))?.is_file(),
        ".git/HEAD must be a file"
    );

    file_contents(
        dir.subpath(".git/HEAD"),
        predicate::str::is_match("^ref: refs/heads/(main|master)\n$").unwrap(),
    );

    Ok(())
}
