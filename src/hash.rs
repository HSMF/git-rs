use std::{fmt::Display, io::Cursor, path::PathBuf, str::FromStr};

use itertools::Itertools;
use sha1::{Digest, Sha1};

use crate::{PathBufExt, Writeable};

#[derive(Clone)]
pub struct Hash {
    buf: [u8; 20],
}

impl std::fmt::Debug for Hash {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Hash({self})")
    }
}

impl Writeable for Hash {
    /// writes the hash in compact form
    fn fmt<W: std::io::Write>(&self, f: &mut W) -> std::io::Result<()> {
        f.write_all(self.buf.as_slice())?;
        Ok(())
    }
}

impl Hash {
    pub fn from_raw(b: &[u8]) -> Option<Self> {
        if b.len() != 20 {
            return None;
        }

        let mut buf = [0; 20];
        for (i, b) in b.iter().enumerate() {
            buf[i] = *b;
        }
        Some(Self { buf })
    }

    pub fn from_bytes(b: &[u8]) -> Self {
        let mut hasher = Sha1::new();
        hasher.update(b);
        let result = hasher.finalize();
        let mut buf = [0; 20];
        for (i, byte) in result.into_iter().enumerate() {
            buf[i] = byte;
        }

        Self { buf }
    }

    pub fn from_writable(x: impl Writeable) -> Hash {
        let mut buf = Cursor::new(Vec::new());
        x.fmt(&mut buf).unwrap();
        Hash::from_bytes(buf.into_inner().as_slice())
    }
}

#[derive(Debug, derive_more::Display, Clone, thiserror::Error)]
pub enum HashError {
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

    pub fn dir(&self) -> PathBuf {
        let s = self.to_string();
        PathBuf::new().push_dir(&s[..2])
    }
}
