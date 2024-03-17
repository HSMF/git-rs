use anyhow::{bail, Context};
use clap::{ArgGroup, Parser, Subcommand, ValueEnum};
use hash::Hash;
use itertools::Itertools;
use object::{Blob, Object, Tree, ZlibReadExt, ZlibWriter};
use std::{
    fmt::Debug,
    fs::{create_dir, File},
    io::{self, stdout, BufRead, BufReader, Write},
    path::{Path, PathBuf},
    process::ExitCode,
};
use walkdir::WalkDir;

use crate::object::{Commit, Event};
mod hash;
mod object;

pub fn root() -> PathBuf {
    ".git".into()
}

trait PathBufExt {
    fn push_dir<P: AsRef<Path>>(self, path: P) -> Self;
}

impl PathBufExt for PathBuf {
    fn push_dir<P: AsRef<Path>>(mut self, path: P) -> Self {
        self.push(path);
        self
    }
}

trait IoErrorExt {
    type Item;
    fn ignore(self, kind: std::io::ErrorKind, default: Self::Item) -> Self;
}

impl<T> IoErrorExt for io::Result<T> {
    type Item = T;
    fn ignore(self, kind: std::io::ErrorKind, default: Self::Item) -> Self {
        self.or_else(|x| {
            if x.kind() == kind {
                Ok(default)
            } else {
                Err(x)
            }
        })
    }
}

#[derive(Debug, thiserror::Error)]
pub enum ReadError<E> {
    IoError(std::io::Error),
    ParseError(E),
}

impl<E> std::fmt::Display for ReadError<E>
where
    E: Debug,
{
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{self:?}")
    }
}

pub trait Writeable {
    fn fmt<W: std::io::Write>(&self, f: &mut W) -> std::io::Result<()>;
}

pub trait Readable {
    type Error;
    fn read<R: std::io::Read>(r: R) -> Result<Self, ReadError<Self::Error>>
    where
        Self: Sized;
}

impl<T> Writeable for &T
where
    T: Writeable,
{
    fn fmt<W: std::io::Write>(&self, f: &mut W) -> std::io::Result<()> {
        (*self).fmt(f)
    }
}

#[derive(Debug, Parser)]
struct Cli {
    #[clap(subcommand)]
    subcommand: Command,
}

#[derive(Subcommand, Debug)]
enum Command {
    Init,
    CatFile {
        /// pretty-prints object
        #[clap(short = 'p', group = "mode")]
        pretty: bool,

        /// tests if the object with the given hash exists
        #[clap(short = 'e', group = "mode")]
        exists: bool,

        #[clap(requires = "mode")]
        object: String,
    },
    #[clap(group(ArgGroup::new("input").required(true).args(&["file", "stdin"])  ))]
    HashObject {
        /// Writes the object back to the store
        #[clap(short)]
        write: bool,
        /// Type of the object
        #[clap(short, value_enum)]
        typ: Option<BlobType>,
        /// Read from stdin instead of file
        #[clap(long)]
        stdin: bool,

        /// The file
        file: Option<String>,
    },

    LsTree {
        #[clap(long, group = "only")]
        name_only: bool,
        #[clap(long, group = "only")]
        object_only: bool,
        #[clap(short)]
        recursive: bool,

        tree_hash: String,
    },

    WriteTree {},

    CommitTree {
        #[clap(short)]
        parent: Vec<Hash>,
        #[clap(short)]
        message: Vec<String>,

        tree: Hash,
    },
}

#[derive(Copy, Clone, PartialEq, Eq, PartialOrd, Ord, ValueEnum, Debug, Default)]
enum BlobType {
    #[default]
    Blob,
    Commit,
    Tree,
    Tag,
}

pub fn init() -> anyhow::Result<()> {
    let default_branch = "main";
    std::fs::create_dir(".git")?;
    std::fs::create_dir(".git/objects")?;
    std::fs::create_dir(".git/refs")?;
    let mut f = std::fs::File::create(".git/HEAD")?;
    writeln!(f, "ref: refs/heads/{default_branch}")?;
    Ok(())
}

pub struct CatFile {
    hash: Hash,
}

impl CatFile {
    pub fn new(hash: &str) -> anyhow::Result<Self> {
        let hash: Hash = hash.parse().context("failed to parse hash")?;
        Ok(Self { hash })
    }

    fn path(&self) -> PathBuf {
        root().push_dir("objects").push_dir(self.hash.object_path())
    }

    pub fn exists(&self) -> anyhow::Result<bool> {
        let metadata = std::fs::metadata(self.path());
        match metadata {
            Ok(m) => {
                if m.is_file() {
                    Ok(true)
                } else {
                    bail!("path exists but isn't a file");
                }
            }
            Err(e) => {
                if e.kind() == std::io::ErrorKind::NotFound {
                    Ok(false)
                } else {
                    Err(e)?
                }
            }
        }
    }

    pub fn pretty(&self) -> anyhow::Result<()> {
        let path = self.path();

        let mut f = File::open(path)?;

        // TODO: object, not blob
        let blob: Blob = f.zlib_read()?;

        stdout().lock().write_all(blob.content())?;

        Ok(())
    }
}

pub struct HashObject {
    object: Object,
}

impl HashObject {
    pub fn new(object: Object) -> Self {
        Self { object }
    }

    pub fn write(&self) -> anyhow::Result<()> {
        // hash will be computed twice
        // question: do i care?
        let hash = self.hash();

        create_dir(root().push_dir("objects").push_dir(hash.dir()))
            .ignore(std::io::ErrorKind::AlreadyExists, ())?;
        let path = root().push_dir("objects").push_dir(hash.object_path());
        let mut file = File::create(path).context("failed to create object file")?;

        let obj = ZlibWriter::new(&self.object);
        obj.fmt(&mut file)?;

        Ok(())
    }

    pub fn hash(&self) -> Hash {
        let s = self.object.to_string();
        Hash::from_bytes(s.as_bytes())
    }
}

fn main() -> anyhow::Result<ExitCode> {
    let cli = Cli::parse();
    match cli.subcommand {
        Command::Init => init()?,
        Command::CatFile {
            pretty,
            exists,
            object,
        } => {
            let cat_file = CatFile::new(&object)?;
            if pretty {
                cat_file.pretty()?;
            }
            if exists {
                if cat_file.exists()? {
                    return Ok(ExitCode::SUCCESS);
                } else {
                    return Ok(ExitCode::FAILURE);
                }
            }
        }
        Command::HashObject {
            write,
            typ: _,
            stdin,
            file,
        } => {
            let source: Box<dyn BufRead> = if stdin {
                Box::new(BufReader::new(io::stdin().lock()))
            } else {
                let file = file.expect("guaranteed to not be none");
                let file = File::open(file)?;
                Box::new(BufReader::new(file))
            };

            let cmd = HashObject::new(Object::new_blob(source)?);

            if write {
                cmd.write()?;
            }

            println!("{}", cmd.hash());
        }

        Command::LsTree {
            name_only,
            object_only,
            tree_hash,
            recursive,
        } => {
            let hash: Hash = tree_hash.parse()?;
            let path = root().push_dir("objects").push_dir(hash.object_path());

            let mut f = File::open(path)?;
            let tree: Tree = f.zlib_read()?;

            let mut printer = tree.display();
            if recursive {
                printer.recusive();
            }
            if name_only {
                printer.no_type();
                printer.no_perms();
                printer.no_object();
            }
            if object_only {
                printer.no_type();
                printer.no_perms();
                printer.no_name();
            }

            print!("{}", printer);
        }

        Command::WriteTree {} => {
            let (ok, err): (Vec<_>, Vec<_>) = WalkDir::new(".")
                .into_iter()
                .filter_entry(|e| e.file_name() != ".git")
                .partition_result();
            for e in err {
                eprintln!("Error: {e}");
            }
            let tree = Tree::write_tree(ok.into_iter())?;
            println!("{}", tree);
        }

        Command::CommitTree {
            parent,
            message,
            tree,
        } => {
            let author = Event::new(
                "hello world".to_owned(),
                "hello.world@example.com".to_owned(),
            );
            let committer = Event::new(
                "hello world".to_owned(),
                "hello.world@example.com".to_owned(),
            );
            let message = message.join(" ");
            let commit = Commit::new(tree, &message, author, committer, parent)?;
            dbg!(&commit);
            let id = Hash::from_writable(&commit);
            let mut file = File::create(Object::path(&id)?)?;
            ZlibWriter::new(commit).fmt(&mut file)?;
                println!("{id}");
        }
    }
    Ok(ExitCode::SUCCESS)
}
