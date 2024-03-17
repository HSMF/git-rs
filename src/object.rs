use std::io::{Cursor, Read};
use std::{
    collections::HashMap,
    ffi::OsString,
    fmt::Display,
    fs::{create_dir, File},
    io::BufRead,
    os::unix::{ffi::OsStringExt, fs::PermissionsExt},
    path::{Path, PathBuf},
};

use flate2::read::ZlibDecoder;
use flate2::write::ZlibEncoder;
use flate2::Compression;
use nom::{
    bytes::complete::{tag, take_until},
    character::complete::{digit1, oct_digit1},
    IResult, ParseTo,
};
use walkdir::DirEntry;

use crate::Writeable;
use crate::{hash::Hash, root, IoErrorExt, PathBufExt};

fn hash(x: impl Writeable) -> Hash {
    let mut buf = Cursor::new(Vec::new());
    x.fmt(&mut buf).unwrap();
    Hash::from_bytes(buf.into_inner().as_slice())
}

pub struct ZlibWriter<T>(T);

impl<T> ZlibWriter<T> {
    pub fn new(x: T) -> Self {
        Self(x)
    }
}

impl<T> Writeable for ZlibWriter<T>
where
    T: Writeable,
{
    fn fmt<W: std::io::Write>(&self, f: &mut W) -> std::io::Result<()> {
        let mut encoder = ZlibEncoder::new(Vec::new(), Compression::default());
        self.0.fmt(&mut encoder)?;

        let compressed = encoder.finish()?;
        f.write_all(&compressed)?;

        Ok(())
    }
}

#[derive(Debug, Clone)]
pub struct Blob {
    content: Vec<u8>,
}

impl Blob {
    pub fn new(content: Vec<u8>) -> Self {
        Self { content }
    }

    pub fn content(&self) -> &[u8] {
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

impl Writeable for Blob {
    fn fmt<W: std::io::Write>(&self, f: &mut W) -> std::io::Result<()> {
        write!(f, "blob {}\0", self.content.len())?;
        f.write_all(&self.content)?;
        Ok(())
    }
}

#[derive(Debug, derive_more::Display, Clone, thiserror::Error)]
pub enum ParseError {
    FormatError,
    LengthMismatch,
}

impl TryFrom<&[u8]> for Blob {
    type Error = ParseError;

    fn try_from(value: &[u8]) -> Result<Self, Self::Error> {
        fn parser(s: &[u8]) -> IResult<&[u8], &[u8]> {
            let (s, _) = tag("blob ")(s)?;
            let (s, len) = digit1(s)?;
            let (s, _) = tag("\0")(s)?;
            let err = nom::Err::Failure(nom::error::Error::new(s, nom::error::ErrorKind::Digit));
            let len: usize = len.parse_to().ok_or(err)?;
            let (rest, blob) = nom::bytes::complete::take(len)(s)?;
            Ok((rest, blob))
        }
        let (rest, blob) = parser(value).map_err(|_| ParseError::FormatError)?;
        if !rest.is_empty() {
            return Err(ParseError::LengthMismatch);
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

impl Writeable for Object {
    fn fmt<W: std::io::Write>(&self, f: &mut W) -> std::io::Result<()> {
        match self {
            Object::Blob(b) => <Blob as Writeable>::fmt(b, f)?,
            Object::Tree(t) => <Tree as Writeable>::fmt(t, f)?,
        }

        Ok(())
    }
}

impl Object {
    pub fn new_blob(mut source: impl BufRead) -> anyhow::Result<Self> {
        let mut buf = Vec::new();
        source.read_to_end(&mut buf)?;
        let object = Object::Blob(Blob::new(buf));
        Ok(object)
    }

    pub fn hash(&self) -> Hash {
        let s = self.to_string();
        Hash::from_bytes(s.as_bytes())
    }
}

const REGULAR_FILE: u32 = 0o100644;
const EXECUTABLE_FILE: u32 = 0o100755;
const SYMBOLIC_LINK: u32 = 0o120000;
const DIRECTORY: u32 = 0o040000;

#[repr(u32)]
#[derive(Debug, PartialEq, Eq, Clone, Copy)]
enum Perms {
    RegularFile = REGULAR_FILE,
    ExecutableFile = EXECUTABLE_FILE,
    SymbolicLink = SYMBOLIC_LINK,
    Directory = DIRECTORY,
}

impl Perms {
    fn rendered_size(&self) -> usize {
        let me = *self as u32;
        let size = (u32::BITS - me.leading_zeros() + 2) / 3;

        size as usize
    }
}

#[cfg(test)]
mod perms_size {
    use super::*;
    #[test]
    fn rendered_size() {
        assert_eq!(Perms::ExecutableFile.rendered_size(), 6);
        assert_eq!(Perms::RegularFile.rendered_size(), 6);
        assert_eq!(Perms::SymbolicLink.rendered_size(), 6);
        assert_eq!(Perms::Directory.rendered_size(), 5);
    }
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
        write!(f, "{}", self.display())
    }
}

impl Writeable for Tree {
    fn fmt<W: std::io::Write>(&self, f: &mut W) -> std::io::Result<()> {
        let size: usize = self
            .entries
            .iter()
            .map(|x| x.perms.rendered_size() + 1 + x.name.len() + 1 + 20)
            .sum();
        write!(f, "tree {size}\0")?;
        for entry in &self.entries {
            write!(f, "{:o} ", entry.perms as u32)?;
            let b = entry.name.as_encoded_bytes();
            f.write_all(b)?;
            write!(f, "\0")?;
            <Hash as Writeable>::fmt(&entry.hash, f)?;
        }

        Ok(())
    }
}

impl TryFrom<&[u8]> for Tree {
    type Error = ParseError;
    fn try_from(s: &[u8]) -> Result<Self, Self::Error> {
        fn header(s: &[u8]) -> IResult<&[u8], &[u8]> {
            let (s, _) = tag("tree ")(s)?;
            let (s, len) = digit1(s)?;
            let err = nom::Err::Failure(nom::error::Error::new(s, nom::error::ErrorKind::Digit));
            let len: usize = len.parse_to().ok_or(err)?;
            let (s, _) = tag("\0")(s)?;

            let (s, body) = nom::bytes::complete::take(len)(s)?;
            Ok((s, body))
        }

        fn entry(s: &[u8]) -> IResult<&[u8], TreeEntry> {
            let (s, perm) = oct_digit1(s)?;
            let (s, _) = tag(" ")(s)?;
            fn err(s: &[u8]) -> nom::Err<nom::error::Error<&[u8]>> {
                nom::Err::Failure(nom::error::Error::new(s, nom::error::ErrorKind::Digit))
            }
            fn parse_perm(perm: &[u8]) -> Option<u32> {
                if perm.len() > 10 {
                    return None;
                }

                Some(perm.iter().fold(0, |acc, digit| {
                    let digit = (digit - b'0') as u32;
                    acc * 8 + digit
                }))
            }
            let perm: u32 = parse_perm(perm).ok_or_else(|| err(s))?;
            let perms = if perm == REGULAR_FILE {
                Perms::RegularFile
            } else if perm == EXECUTABLE_FILE {
                Perms::ExecutableFile
            } else if perm == SYMBOLIC_LINK {
                Perms::SymbolicLink
            } else if perm == DIRECTORY {
                Perms::Directory
            } else {
                return Err(err(s));
            };

            let (s, name) = take_until("\0")(s)?;
            let (s, _) = tag("\0")(s)?;
            let name = OsString::from_vec(name.to_owned());

            let (s, hash) = nom::bytes::complete::take(20usize)(s)?;
            let hash = Hash::from_raw(hash).unwrap();

            Ok((s, TreeEntry { perms, name, hash }))
        }

        let (rest, mut body) = header(s).map_err(|_| ParseError::FormatError)?;
        if !rest.is_empty() {
            Err(ParseError::LengthMismatch)?;
        }
        let mut entries = vec![];

        while !body.is_empty() {
            let (s, entry) = entry(body).map_err(|_| ParseError::FormatError)?;
            entries.push(entry);
            body = s;
        }

        Ok(Self { entries })
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
            recurse: false,
            prefix: PathBuf::new(),
        }
    }

    pub fn write_tree<I>(files: I) -> anyhow::Result<Hash>
    where
        I: Iterator<Item = DirEntry>,
    {
        // this is with an iterator to implement a staging area later
        let mut collection = HashMap::<_, Vec<_>>::new();
        for file in files {
            let Some(dirname) = file.path().parent() else {
                continue;
            };
            if dirname.as_os_str() == "" {
                continue;
            }
            let entry = collection.entry(dirname.to_owned()).or_default();
            entry.push(file);
        }

        fn foo(
            map: &HashMap<PathBuf, Vec<DirEntry>>,
            trees: &mut Vec<Tree>,
            current: &Path,
        ) -> anyhow::Result<Hash> {
            let entries = map.get(current).expect("current is in graph");
            let mut children = vec![];
            for entry in entries {
                let hash = if entry.file_type().is_dir() {
                    foo(map, trees, entry.path())?
                } else {
                    Object::Blob(Blob::new(std::fs::read(entry.path())?)).hash()
                };
                let perms = if entry.file_type().is_dir() {
                    Perms::Directory
                } else if entry.path_is_symlink() {
                    Perms::SymbolicLink
                } else if entry.metadata()?.permissions().mode() & 0o111 != 0 {
                    Perms::ExecutableFile
                } else {
                    Perms::RegularFile
                };
                children.push(TreeEntry {
                    name: entry.file_name().to_owned(),
                    hash,
                    perms,
                })
            }

            let tree = Tree { entries: children };
            let hash = hash(&tree);
            trees.push(tree);
            Ok(hash)
        }

        let mut trees = vec![];
        let hashed = foo(&collection, &mut trees, PathBuf::from(".").as_path())?;

        for tree in trees {
            let hashed = hash(&tree);
            dbg!(&hashed);
            let path = root().push_dir("objects").push_dir(hashed.object_path());
            create_dir(path.parent().unwrap()).ignore(std::io::ErrorKind::AlreadyExists, ())?;
            let mut f = File::create(path)?;
            let writer = ZlibWriter::new(&tree);
            writer.fmt(&mut f)?;
        }

        Ok(hashed)
    }
}

pub struct TreePrinter<'a> {
    tree: &'a Tree,
    show_name: bool,
    show_perms: bool,
    show_type: bool,
    recurse: bool,
    show_object: bool,
    prefix: PathBuf,
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

    pub fn recusive(&mut self) -> &mut Self {
        self.recurse = true;
        self
    }
}

fn print_name(f: &mut std::fmt::Formatter<'_>, name: &Path) -> std::fmt::Result {
    write!(f, "{}", name.display())?;
    Ok(())
}

impl TreePrinter<'_> {}

impl Display for TreePrinter<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        fn write_sep(f: &mut std::fmt::Formatter<'_>, sep: &str, needed: bool) -> std::fmt::Result {
            if needed {
                write!(f, "{sep}")?;
            }

            Ok(())
        }
        for entry in &self.tree.entries {
            let name = self.prefix.to_owned().push_dir(&entry.name);
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
                print_name(f, &name)?;
                // need_sep = true;
            }

            writeln!(f)?;

            if self.recurse && entry.perms == Perms::Directory {
                let path = root()
                    .push_dir("objects")
                    .push_dir(entry.hash.object_path());
                let Ok(data) = std::fs::read(path) else {
                    continue;
                };
                let mut decoder = ZlibDecoder::new(data.as_slice());
                let mut contents = Vec::new();
                let Ok(_) = decoder.read_to_end(&mut contents) else {
                    continue;
                };
                let Ok(tree): Result<Tree, _> = contents.as_slice().try_into() else {
                    continue;
                };
                let tree = &tree;

                let print = TreePrinter {
                    tree,
                    prefix: name,
                    ..*self
                };
                write!(f, "{print}")?;
            }
        }

        Ok(())
    }
}
