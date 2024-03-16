use std::{
    ffi::{OsStr, OsString},
    fmt::Display,
};

use nom::{bytes::complete::tag, character::complete::digit1, IResult, ParseTo};

use crate::hash::Hash;

#[derive(Debug, Clone)]
pub struct Blob {
    content: Vec<u8>,
}

impl Blob {
    pub fn new(content: Vec<u8>) -> Self {
        Self { content }
    }

    pub(crate) fn content(&self) -> &[u8] {
        &self.content
    }
}

impl Display for Blob {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "blob {}\0", self.content.len())?;
        for &ch in &self.content {
            write!(f, "{}", ch as char)?;
        }
        Ok(())
    }
}

#[derive(Debug, derive_more::Display, Clone, thiserror::Error)]
pub enum BlobError {
    FormatError,
    LengthMismatch,
}

impl TryFrom<&[u8]> for Blob {
    type Error = BlobError;

    fn try_from(value: &[u8]) -> Result<Self, Self::Error> {
        fn parser(s: &[u8]) -> IResult<&[u8], &[u8]> {
            let (s, _) = tag("blob ")(s)?;
            let (s, len) = digit1(s)?;
            let (s, _) = tag("\0")(s)?;
            let len: usize = len.parse_to().unwrap();
            let (rest, blob) = nom::bytes::complete::take(len)(s)?;
            Ok((rest, blob))
        }
        let (rest, blob) = parser(value).map_err(|_| BlobError::FormatError)?;
        if !rest.is_empty() {
            return Err(BlobError::LengthMismatch);
        }
        Ok(Blob {
            content: blob.to_vec(),
        })
    }
}

#[derive(Debug, derive_more::Display)]
pub enum Object {
    Blob(Blob),
    Tree(Tree),
}

const REGULAR_FILE: u32 = 0o100644;
const EXECUTABLE_FILE: u32 = 0o100755;
const SYMBOLIC_LINK: u32 = 0o120000;
const DIRECTORY: u32 = 0o400000;

#[repr(u32)]
#[derive(Debug, PartialEq, Eq, Clone, Copy)]
enum Perms {
    RegularFile = REGULAR_FILE,
    ExecutableFile = EXECUTABLE_FILE,
    SymbolicLink = SYMBOLIC_LINK,
    Directory = DIRECTORY,
}

#[derive(Debug)]
pub struct Tree {
    entries: Vec<TreeEntry>,
}

#[derive(Debug)]
struct TreeEntry {
    perms: Perms,
    name: OsString,
    hash: Hash,
}

impl Display for Tree {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        todo!()
    }
}

impl Tree {
    pub fn display(&self) -> TreePrinter {
        TreePrinter {
            tree: self,
            show_name: true,
            show_perms: true,
            show_object: true,
            show_type: true,
        }
    }
}

pub struct TreePrinter<'a> {
    tree: &'a Tree,
    show_name: bool,
    show_perms: bool,
    show_type: bool,
    // recurse: bool,
    show_object: bool,
}

impl TreePrinter<'_> {
    pub fn no_name(&mut self) -> &mut Self {
        self.show_name = false;
        self
    }

    pub fn no_perms(&mut self) -> &mut Self {
        self.show_perms = false;
        self
    }

    pub fn no_object(&mut self) -> &mut Self {
        self.show_object = false;
        self
    }

    pub fn no_type(&mut self) -> &mut Self {
        self.show_type = false;
        self
    }
}

fn print_name(f: &mut std::fmt::Formatter<'_>, name: &OsStr) -> std::fmt::Result {
    writeln!(f, "{}", name.to_string_lossy())?;
    Ok(())
}

impl Display for TreePrinter<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        fn write_sep(f: &mut std::fmt::Formatter<'_>, sep: &str, needed: bool) -> std::fmt::Result {
            if needed {
                write!(f, "{sep}")?;
            }

            Ok(())
        }
        for entry in &self.tree.entries {
            let mut need_sep = false;

            if self.show_perms {
                write_sep(f, " ", need_sep)?;
                write!(f, "{:06o}", entry.perms as u32)?;
                need_sep = true;
            }

            if self.show_type {
                write_sep(f, " ", need_sep)?;
                let typ = match entry.perms {
                    Perms::Directory => "tree",
                    _ => "blob",
                };
                write!(f, "{typ}")?;
                need_sep = true;
            }

            if self.show_object {
                write_sep(f, " ", need_sep)?;
                write!(f, "{}", entry.hash)?;
                need_sep = true;
            }

            if self.show_name {
                write_sep(f, " ", need_sep)?;
                print_name(f, &entry.name)?;
                // need_sep = true;
            }

            writeln!(f)?;
        }

        Ok(())
    }
}
