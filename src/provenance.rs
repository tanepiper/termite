//! A log of what termite installed: hashes, never contents. Each entry holds
//! the hash of the one before it and a keyed signature, so editing, removing
//! or reordering an entry breaks the chain. Contract: requests are stored only
//! as hashes, and a log that doesn't verify is never added to.

use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};
use std::{fs, io};

use hmac::{Hmac, KeyInit, Mac};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use thiserror::Error;

use crate::home::{HomeError, PRIVATE_FILE_MODE, write_private};

/// The log's file name, in termite's folder.
pub const PROVENANCE_FILE: &str = "provenance.jsonl";

/// What happened.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Event {
    Drafted,
    Installed,
    ChangedAccepted,
    Removed,
    LibraryUpdated,
    Alert,
}

/// One line of the log.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Entry {
    pub seq: u64,
    pub time: String,
    pub event: Event,
    /// Relative to termite's folder. Empty for an alert.
    pub path: String,
    /// The file's hash, or the request's for an alert.
    pub sha256: String,
    pub flags: Vec<String>,
    pub by: String,
    pub verdict: Option<String>,
    /// The hash of the line before.
    pub prev: String,
    pub mac: String,
}

/// What to add to the log.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Record {
    pub event: Event,
    pub path: String,
    pub sha256: String,
    pub flags: Vec<String>,
    pub by: String,
    pub verdict: Option<String>,
}

impl Record {
    /// An alert. Only the request's hash is kept, never its text.
    pub fn alert(
        request: &str,
        flags: Vec<String>,
        verdict: &str,
        by: &str,
    ) -> Self {
        Self {
            event: Event::Alert,
            path: String::new(),
            sha256: sha256_hex(request.as_bytes()),
            flags,
            by: by.to_owned(),
            verdict: Some(verdict.to_owned()),
        }
    }
}

/// How a file compares with the log.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FileStatus {
    Unchanged,
    Changed,
    /// The log has no current record of it.
    Unknown,
}

#[derive(Debug, Error)]
pub enum ProvenanceError {
    #[error("the provenance log is broken at line {line}: {why}")]
    Broken { line: usize, why: &'static str },
    #[error("cannot read the provenance log: {0}")]
    Read(#[from] io::Error),
    #[error(transparent)]
    Write(#[from] HomeError),
}

/// The SHA-256 of `bytes`, in hex.
pub fn sha256_hex(bytes: &[u8]) -> String {
    hex(&Sha256::digest(bytes))
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

fn signature(key: &[u8], entry: &Entry) -> Option<String> {
    let mut unsigned = entry.clone();
    unsigned.mac = String::new();
    let text = serde_json::to_string(&unsigned).ok()?;
    let mut mac = Hmac::<Sha256>::new_from_slice(key).ok()?;
    mac.update(text.as_bytes());
    Some(hex(&mac.finalize().into_bytes()))
}

/// The log in one folder.
#[derive(Debug, Clone)]
pub struct Log {
    folder: PathBuf,
}

impl Log {
    pub fn at(folder: &Path) -> Self {
        Self {
            folder: folder.to_owned(),
        }
    }

    fn lines(&self) -> Result<Vec<String>, ProvenanceError> {
        match fs::read_to_string(self.folder.join(PROVENANCE_FILE)) {
            Ok(text) => Ok(text.lines().map(str::to_owned).collect()),
            Err(error) if error.kind() == io::ErrorKind::NotFound => {
                Ok(Vec::new())
            },
            Err(error) => Err(error.into()),
        }
    }

    /// Every entry that can be read. Needs no key, so don't treat this as
    /// proof: only [`Log::verify`] is.
    pub fn entries(&self) -> Result<Vec<Entry>, ProvenanceError> {
        Ok(self
            .lines()?
            .iter()
            .filter_map(|line| serde_json::from_str(line).ok())
            .collect())
    }

    /// Check the whole chain. Returns how many entries there are. Removing the
    /// last entries, or the whole file, can't be seen from here.
    pub fn verify(&self, key: &[u8]) -> Result<usize, ProvenanceError> {
        let lines = self.lines()?;
        let mut prev = String::new();
        for (index, line) in lines.iter().enumerate() {
            let broken = |why| ProvenanceError::Broken {
                line: index + 1,
                why,
            };
            let entry: Entry = serde_json::from_str(line)
                .map_err(|_| broken("it can't be read"))?;
            if entry.seq != index as u64 + 1 {
                return Err(broken("an entry is missing or out of order"));
            }
            if entry.prev != prev {
                return Err(broken("it doesn't follow the entry before it"));
            }
            if signature(key, &entry).as_deref() != Some(entry.mac.as_str()) {
                return Err(broken("its signature is wrong"));
            }
            prev = sha256_hex(line.as_bytes());
        }
        Ok(lines.len())
    }

    /// Add an entry. The log must verify first.
    pub fn append(
        &self,
        key: &[u8],
        record: Record,
    ) -> Result<Entry, ProvenanceError> {
        self.verify(key)?;
        let mut lines = self.lines()?;
        let mut entry = Entry {
            seq: lines.len() as u64 + 1,
            time: utc_time(now()),
            event: record.event,
            path: record.path,
            sha256: record.sha256,
            flags: record.flags,
            by: record.by,
            verdict: record.verdict,
            prev: lines
                .last()
                .map(|line| sha256_hex(line.as_bytes()))
                .unwrap_or_default(),
            mac: String::new(),
        };
        entry.mac = signature(key, &entry).unwrap_or_default();
        let line = serde_json::to_string(&entry).unwrap_or_default();
        lines.push(line);
        let text = lines.join("\n") + "\n";
        write_private(
            &self.folder,
            PROVENANCE_FILE,
            text.as_bytes(),
            PRIVATE_FILE_MODE,
        )?;
        Ok(entry)
    }

    /// Compare a file with its last entry.
    pub fn status(
        &self,
        path: &str,
        bytes: &[u8],
    ) -> Result<FileStatus, ProvenanceError> {
        let entries = self.entries()?;
        let last = entries.iter().rev().find(|entry| {
            entry.path == path
                && matches!(
                    entry.event,
                    Event::Installed
                        | Event::ChangedAccepted
                        | Event::LibraryUpdated
                        | Event::Removed
                )
        });
        Ok(match last {
            None => FileStatus::Unknown,
            Some(entry) if entry.event == Event::Removed => FileStatus::Unknown,
            Some(entry) if entry.sha256 == sha256_hex(bytes) => {
                FileStatus::Unchanged
            },
            Some(_) => FileStatus::Changed,
        })
    }
}

fn now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |elapsed| elapsed.as_secs())
}

/// Seconds since 1970 as a UTC time, such as `2026-11-04T19:20:00Z`.
fn utc_time(seconds: u64) -> String {
    let (days, rest) = (seconds / 86_400, seconds % 86_400);
    // Howard Hinnant's civil_from_days, for dates from 1970 on.
    let z = days + 719_468;
    let era = z / 146_097;
    let day_of_era = z % 146_097;
    let year_of_era = (day_of_era - day_of_era / 1_460 + day_of_era / 36_524
        - day_of_era / 146_096)
        / 365;
    let day_of_year =
        day_of_era - (365 * year_of_era + year_of_era / 4 - year_of_era / 100);
    let shifted_month = (5 * day_of_year + 2) / 153;
    let day = day_of_year - (153 * shifted_month + 2) / 5 + 1;
    let month = if shifted_month < 10 {
        shifted_month + 3
    } else {
        shifted_month - 9
    };
    let year = year_of_era + era * 400 + u64::from(month <= 2);
    format!(
        "{year:04}-{month:02}-{day:02}T{:02}:{:02}:{:02}Z",
        rest / 3_600,
        rest % 3_600 / 60,
        rest % 60
    )
}

#[cfg(test)]
mod tests;
