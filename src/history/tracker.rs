use std::collections::HashMap;
use std::fs::{File, OpenOptions};
use std::io::{BufRead, BufReader, Write};
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

use crate::error::TsmError;

use crate::error::Result;

/// Read/write access to window access-history.
///
/// Commands depend on this trait rather than the concrete [`WindowHistory`], so
/// their history logic can be tested against an in-memory store with
/// deterministic timestamps — no filesystem, no wall-clock.
pub trait HistoryStore {
    /// Most-recent access timestamp for a window, if it has one.
    fn last_access(&self, session: &str, window_index: u32) -> Option<u128>;

    /// Record that a window was just accessed, persisting the change.
    fn record(&mut self, session: &str, window_index: u32) -> Result<()>;
}

pub struct WindowHistory {
    file_path: PathBuf,
    entries: HashMap<String, u128>,
}

impl WindowHistory {
    pub fn new(file_path: PathBuf) -> Self {
        Self {
            file_path,
            entries: HashMap::new(),
        }
    }

    /// Construct and load history from `file_path` in one step.
    pub fn open(file_path: PathBuf) -> Result<Self> {
        let mut history = Self::new(file_path);
        history.load()?;
        Ok(history)
    }

    pub fn load(&mut self) -> Result<()> {
        if self.file_path.exists() {
            let file = File::open(&self.file_path)?;
            let reader = BufReader::new(file);

            for line in reader.lines() {
                let line = line?;
                let parts: Vec<&str> = line.split('\t').collect();
                if parts.len() == 2
                    && let Ok(timestamp) = parts[1].parse::<u128>()
                {
                    self.entries.insert(parts[0].to_string(), timestamp);
                }
            }
        }
        Ok(())
    }

    pub fn save(&self) -> Result<()> {
        let mut entries: Vec<_> = self.entries.iter().collect();
        entries.sort_by(|a, b| b.1.cmp(a.1)); // Sort by timestamp descending
        entries.truncate(100);

        // Acquire lock before opening file to prevent race conditions
        #[cfg(unix)]
        let _guard = {
            let lock_path = self.file_path.with_extension("lock");
            let mut attempts = 0;
            loop {
                match OpenOptions::new()
                    .write(true)
                    .create_new(true) // Fails if file already exists
                    .open(&lock_path)
                {
                    Ok(_) => break LockGuard { path: lock_path },
                    Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists && attempts < 10 => {
                        std::thread::sleep(std::time::Duration::from_millis(10));
                        attempts += 1;
                    }
                    Err(e) => return Err(TsmError::Io(e)),
                }
            }
        };

        let mut file = OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(true)
            .open(&self.file_path)?;

        for (window_id, timestamp) in entries {
            writeln!(file, "{}\t{}", window_id, timestamp)?;
        }
        file.sync_all()?;
        Ok(())
    }

    pub fn record_access(&mut self, session: &str, window_index: u32) -> Result<()> {
        let window_id = format!("{}:{}", session, window_index);
        let timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_err(|_| TsmError::Io(std::io::Error::other("System time went backwards")))?
            .as_millis();
        self.entries.insert(window_id, timestamp);
        Ok(())
    }

    pub fn get_last_access(&self, session: &str, window_index: u32) -> Option<u128> {
        let window_id = format!("{}:{}", session, window_index);
        self.entries.get(&window_id).cloned()
    }
}

impl HistoryStore for WindowHistory {
    fn last_access(&self, session: &str, window_index: u32) -> Option<u128> {
        self.get_last_access(session, window_index)
    }

    /// Records the access in memory (stamping the current time) and flushes the
    /// whole history to disk.
    fn record(&mut self, session: &str, window_index: u32) -> Result<()> {
        self.record_access(session, window_index)?;
        self.save()
    }
}

// RAII guard for lock file cleanup
#[cfg(unix)]
struct LockGuard {
    path: PathBuf,
}

#[cfg(unix)]
impl Drop for LockGuard {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.path);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use tempfile::{NamedTempFile, TempDir};

    #[test]
    fn record_and_query_access() {
        let file = NamedTempFile::new().unwrap();
        let mut history = WindowHistory::new(file.path().to_path_buf());

        assert_eq!(history.get_last_access("dev", 1), None);
        history.record_access("dev", 1).unwrap();
        assert!(history.get_last_access("dev", 1).is_some());
        // A different window index is a distinct key.
        assert_eq!(history.get_last_access("dev", 2), None);
    }

    #[test]
    fn record_access_overwrites_timestamp_for_same_window() {
        let file = NamedTempFile::new().unwrap();
        let mut history = WindowHistory::new(file.path().to_path_buf());
        history.record_access("dev", 1).unwrap();
        let first = history.get_last_access("dev", 1).unwrap();
        // Later record for the same key replaces, never duplicates.
        std::thread::sleep(std::time::Duration::from_millis(2));
        history.record_access("dev", 1).unwrap();
        let second = history.get_last_access("dev", 1).unwrap();
        assert!(second >= first);
    }

    #[test]
    fn save_then_load_round_trips_entries() {
        let file = NamedTempFile::new().unwrap();
        let path = file.path().to_path_buf();

        let mut writer = WindowHistory::new(path.clone());
        writer.record_access("a", 0).unwrap();
        writer.record_access("b", 3).unwrap();
        writer.save().unwrap();

        let mut reader = WindowHistory::new(path);
        reader.load().unwrap();
        assert!(reader.get_last_access("a", 0).is_some());
        assert!(reader.get_last_access("b", 3).is_some());
    }

    #[test]
    fn load_missing_file_is_ok_and_empty() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("does-not-exist");
        let mut history = WindowHistory::new(path);
        history.load().unwrap();
        assert_eq!(history.get_last_access("x", 0), None);
    }

    #[test]
    fn load_skips_malformed_lines() {
        let mut file = NamedTempFile::new().unwrap();
        writeln!(file, "good:1\t12345").unwrap();
        writeln!(file, "missing-timestamp").unwrap();
        writeln!(file, "bad:2\tnot-a-number").unwrap();
        writeln!(file, "too\tmany\tfields").unwrap();
        writeln!(file, "good:2\t67890").unwrap();
        file.flush().unwrap();

        let mut history = WindowHistory::new(file.path().to_path_buf());
        history.load().unwrap();

        assert_eq!(history.get_last_access("good", 1), Some(12345));
        assert_eq!(history.get_last_access("good", 2), Some(67890));
        assert_eq!(history.get_last_access("bad", 2), None);
    }

    #[test]
    fn save_keeps_only_the_hundred_most_recent_entries() {
        let file = NamedTempFile::new().unwrap();
        let path = file.path().to_path_buf();

        let mut history = WindowHistory::new(path.clone());
        // Insert 150 entries with increasing timestamps by writing directly to
        // the map via record_access is coarse (ms), so seed deterministically.
        for i in 0..150u128 {
            history.entries.insert(format!("s:{}", i), 1_000 + i);
        }
        history.save().unwrap();

        // Reload and count persisted lines: only the 100 newest survive.
        let contents = std::fs::read_to_string(&path).unwrap();
        let lines: Vec<_> = contents.lines().filter(|l| !l.is_empty()).collect();
        assert_eq!(lines.len(), 100);

        let mut reloaded = WindowHistory::new(path);
        reloaded.load().unwrap();
        // The oldest (smallest timestamp) entries were dropped.
        assert_eq!(reloaded.get_last_access("s", 0), None);
        assert_eq!(reloaded.get_last_access("s", 49), None);
        // The newest survive.
        assert_eq!(reloaded.get_last_access("s", 149), Some(1_149));
        assert_eq!(reloaded.get_last_access("s", 50), Some(1_050));
    }

    #[test]
    fn saved_file_is_sorted_by_timestamp_descending() {
        let file = NamedTempFile::new().unwrap();
        let path = file.path().to_path_buf();
        let mut history = WindowHistory::new(path.clone());
        history.entries.insert("s:1".to_string(), 100);
        history.entries.insert("s:2".to_string(), 300);
        history.entries.insert("s:3".to_string(), 200);
        history.save().unwrap();

        let contents = std::fs::read_to_string(&path).unwrap();
        let first_ids: Vec<_> = contents
            .lines()
            .filter(|l| !l.is_empty())
            .map(|l| l.split('\t').next().unwrap().to_string())
            .collect();
        assert_eq!(first_ids, vec!["s:2", "s:3", "s:1"]);
    }
}
