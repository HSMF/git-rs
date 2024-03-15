use anyhow::{bail, Context};
use clap::{Parser, Subcommand};
use flate2::read::ZlibDecoder;
use itertools::Itertools;
use nom::{bytes::complete::tag, character::complete::digit1, IResult, ParseTo};
use std::{
    fmt::Display,
    io::{stdout, Read, Write},
    path::{Path, PathBuf},
    process::ExitCode,
    str::FromStr,
};

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
    HashObject {
        #[clap(short = 'w')]
        file: String,
    },
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

#[derive(Debug, Clone)]
struct Hash {
    buf: [u8; 20],
}

#[derive(Debug, derive_more::Display, Clone, thiserror::Error)]
enum HashError {
    WrongLength,
    UnexpectedChar(char),
}

impl FromStr for Hash {
    type Err = HashError;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        fn to_nibble(ch: char) -> Result<u8, HashError> {
            match ch {
                '0'..='9' => Ok((ch as u8) - b'0'),
                'a'..='f' => Ok((ch as u8) - b'a' + 10),
                _ => Err(HashError::UnexpectedChar(ch)),
            }
        }
        let mut buf = [0; 20];
        if s.len() != 40 {
            return Err(HashError::WrongLength);
        }
        for (i, mut ch) in s.chars().chunks(2).into_iter().enumerate() {
            let first = to_nibble(ch.next().unwrap())?;
            let second = to_nibble(ch.next().unwrap())?;
            buf[i] = first << 4 | second;
        }

        Ok(Self { buf })
    }
}

impl Display for Hash {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        for b in self.buf {
            write!(f, "{b:02x}")?;
        }
        Ok(())
    }
}

impl Hash {
    pub fn object_path(&self) -> PathBuf {
        let s = self.to_string();
        let mut path = PathBuf::new();

        path.push(&s[..2]);
        path.push(&s[2..]);

        path
    }
}

#[derive(Debug, Clone)]
pub struct Blob<'a> {
    content: &'a [u8],
}

impl Display for Blob<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "blob {}\0", self.content.len())?;
        for &ch in self.content {
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

impl<'a> TryFrom<&'a [u8]> for Blob<'a> {
    type Error = BlobError;

    fn try_from(value: &'a [u8]) -> Result<Self, Self::Error> {
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
        Ok(Blob { content: blob })
    }
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
        let blob: Blob = contents.as_slice().try_into()?;

        stdout().lock().write_all(blob.content)?;

        Ok(())
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
        Command::HashObject { .. } => {
            todo!()
        }
    }
    Ok(ExitCode::SUCCESS)
}
