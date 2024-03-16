use std::{fmt::Display, path::PathBuf, str::FromStr};

use itertools::Itertools;
use sha1::{Digest, Sha1};

use crate::PathBufExt;

#[derive(Debug, Clone)]
pub struct Hash {
    buf: [u8; 20],
}

impl Hash {
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

