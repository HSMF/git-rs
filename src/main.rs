use anyhow::{bail, Context};
use clap::{ArgGroup, Parser, Subcommand, ValueEnum};
use flate2::read::ZlibDecoder;
use hash::Hash;
use object::{Blob, Object};
use std::{
    fs::{create_dir, File},
    io::{self, stdout, BufRead, BufReader, Read, Write},
    path::{Path, PathBuf},
    process::ExitCode,
};
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

        let data = std::fs::read(path)?;
        let mut decoder = ZlibDecoder::new(data.as_slice());
        let mut contents = Vec::new();
        decoder.read_to_end(&mut contents)?;
        // TODO: object, not blob
        let blob: Blob = contents.as_slice().try_into()?;

        stdout().lock().write_all(blob.content())?;

        Ok(())
    }
}

pub struct HashObject {
    object: Object,
}

impl HashObject {
    pub fn new_blob(mut source: Box<dyn BufRead>) -> anyhow::Result<Self> {
        let mut buf = Vec::new();
        source.read_to_end(&mut buf)?;
        let object = Object::Blob(Blob::new(buf));

        Ok(Self { object })
    }

    pub fn write(&self) -> anyhow::Result<()> {
        // hash will be computed twice
        // question: do i care?
        let hash = self.hash();

        create_dir(root().push_dir("objects").push_dir(hash.dir())).or_else(|x| {
            match x.kind() {
                std::io::ErrorKind::AlreadyExists => Ok(()),
                _ => Err(x),
            }
        })?;
        let path = root().push_dir("objects").push_dir(hash.object_path());
        let mut file = File::create(path).context("failed to create object file")?;

        write!(file, "{}", self.object)?;

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

            let cmd = HashObject::new_blob(source)?;

            if write {
                cmd.write()?;
            }

            println!("{}", cmd.hash());
        }
    }
    Ok(ExitCode::SUCCESS)
}
