use assert_cmd::prelude::*;
use common::make_dir;
use predicates::prelude::predicate;
use std::{
    fs::{create_dir, File},
    io::Write,
    process::Command,
};

use crate::common::{file_contents, CommandExt};

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

#[test]
fn cat_file() -> anyhow::Result<()> {
    let dir = make_dir();
    dir.git().arg("init").assert().success();

    let sha = ("44", "3aa835bd3a231d1332d1dbc72a014ab29ec0b2");

    create_dir(dir.subpath(format!(".git/objects/{}", sha.0)))?;
    let mut f = File::create(dir.subpath(format!(".git/objects/{}/{}", sha.0, sha.1)))?;
    f.write_all(include_bytes!("./cat-file-input"))?;

    dir.git()
        .args(["cat-file", "-p", &format!("{}{}", sha.0, sha.1)])
        .assert()
        .success()
        .stdout(predicates::str::diff("this is a test!\n"));

    Ok(())
}

#[test]
fn hash_object() -> anyhow::Result<()> {
    let reference_sha = {
        let dir = make_dir();
        Command::new("git")
            .current_dir(dir.path())
            .arg("init")
            .output()?;

        let mut f = File::create(dir.subpath("test.txt"))?;
        writeln!(f, "hello world")?;

        let output = Command::new("git")
            .current_dir(dir.path())
            .args(["hash-object", "-w", "test.txt"])
            .output()?;
        String::from_utf8(output.stdout)?
    };

    let dir = make_dir();
    dir.git().arg("init").assert().success();

    let mut f = File::create(dir.subpath("test.txt"))?;
    writeln!(f, "hello world")?;

    dir.git()
        .args(["hash-object", "-w", "test.txt"])
        .assert()
        .success()
        .stdout(predicates::str::diff(reference_sha));

    Ok(())
}

#[test]
fn ls_tree() -> anyhow::Result<()> {
    let dir = make_dir();
    dir.git().arg("init").assert().success();

    let reference_sha = {
        let real = make_dir();
        create_dir(real.subpath("dir1"))?;
        create_dir(real.subpath("dir2"))?;
        File::create(real.subpath("file0"))?;
        File::create(real.subpath("dir1/file1"))?;
        File::create(real.subpath("dir2/file2"))?;
        File::create(real.subpath("dir2/other_file2"))?;

        real.cmd("git").arg("init").silence().spawn()?.wait()?;
        real.cmd("git")
            .args(["add", "."])
            .silence()
            .spawn()?
            .wait()?;
        let sha = real.cmd("git").arg("write-tree").output()?.stdout;

        real.cmd("cp")
            .args(["-r", ".git/objects"])
            .arg(dir.subpath(".git"))
            .spawn()?
            .wait()?;

        let s = String::from_utf8(sha)?;
        s.trim_end().to_owned()
    };

    println!("{}", dir.path().display());
    dir.git()
        .args(["ls-tree", "--name-only", &reference_sha])
        .assert()
        .success()
        .stdout(predicate::str::diff("dir1\ndir2\nfile0\n"));

    Ok(())
}
