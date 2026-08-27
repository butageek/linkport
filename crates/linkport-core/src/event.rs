//! Append-only JSONL event log used for the portal's history view.

use crate::engine::Outcome;
use chrono::Local;
use serde::{Deserialize, Serialize};
use std::fs::{self, OpenOptions};
use std::io::{BufRead, BufReader, Write};
use std::path::Path;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Event {
    /// RFC 3339 local timestamp.
    pub timestamp: String,
    pub url: String,
    pub host: Option<String>,
    pub outcome: Outcome,
}

impl Event {
    pub fn new(url: impl Into<String>, host: Option<String>, outcome: Outcome) -> Self {
        Self {
            timestamp: Local::now().to_rfc3339(),
            url: url.into(),
            host,
            outcome,
        }
    }
}

pub fn append(path: &Path, event: &Event) -> std::io::Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let mut line = serde_json::to_string(event).map_err(std::io::Error::other)?;
    line.push('\n');
    let mut f = OpenOptions::new().create(true).append(true).open(path)?;
    f.write_all(line.as_bytes())
}

/// Read the most recent `limit` events, newest first. Corrupt lines are
/// skipped. Returns an empty vec when the log does not exist yet.
pub fn read_recent(path: &Path, limit: usize) -> Vec<Event> {
    let Ok(f) = fs::File::open(path) else {
        return Vec::new();
    };
    let mut events: Vec<Event> = BufReader::new(f)
        .lines()
        .map_while(Result::ok)
        .filter_map(|line| serde_json::from_str(&line).ok())
        .collect();
    if events.len() > limit {
        events.drain(..events.len() - limit);
    }
    events.reverse(); // newest first
    events
}

#[cfg(test)]
mod tests {
    use super::*;

    fn path(name: &str) -> std::path::PathBuf {
        std::env::temp_dir().join(format!(
            "linkport-events-{name}-{}.jsonl",
            std::process::id()
        ))
    }

    #[test]
    fn append_and_read_recent() {
        let p = path("basic");
        let _ = fs::remove_file(&p);

        for i in 0..5 {
            append(
                &p,
                &Event::new(
                    format!("https://example.com/{i}"),
                    Some("example.com".into()),
                    Outcome::Default {
                        target: "ff".into(),
                    },
                ),
            )
            .unwrap();
        }
        let recent = read_recent(&p, 3);
        assert_eq!(recent.len(), 3);
        assert_eq!(recent[0].url, "https://example.com/4", "newest first");
        assert_eq!(recent[2].url, "https://example.com/2");
        let _ = fs::remove_file(&p);
    }

    #[test]
    fn read_missing_file_is_empty() {
        assert!(read_recent(path("missing").as_path(), 10).is_empty());
    }
}
