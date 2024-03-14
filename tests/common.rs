use std::{
    fs::read_to_string,
    path::{Path, PathBuf},
};

use predicates::Predicate;
use tempdir::TempDir;

pub struct Temp {
    inner: TempDir,
}

pub fn make_dir() -> Temp {
    Temp::new()
}

impl Temp {
    pub fn new() -> Self {
        Temp {
            inner: TempDir::new("git-rs").expect("could create temp dir"),
        }
    }

    pub fn path(&self) -> &Path {
        self.inner.path()
    }

    pub fn subpath<P: AsRef<Path>>(&self, path: P) -> PathBuf {
        self.inner.path().join(path.as_ref())
    }
}

pub fn file_contents<P: AsRef<Path>>(path: P, pred: impl Predicate<str>) {
    let contents = read_to_string(path.as_ref()).expect("could read file");
    assert!(
        pred.eval(&contents),
        "predicate did not match for {}",
        path.as_ref().display()
    );
}
