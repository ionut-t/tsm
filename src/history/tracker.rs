use std::collections::HashMap;
use std::fs::{File, OpenOptions};
use std::io::{BufRead, BufReader, Write};
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

use crate::error::TsmError;

use crate::error::Result;
use crate::tmux::TmuxClient;

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

    pub fn record_current_window(&mut self, tmux: &TmuxClient) -> Result<()> {
        if !tmux.is_inside_tmux() {
            return Ok(());
        }

        let session = tmux.current_session()?;
        let output = std::process::Command::new("tmux")
            .arg("display-message")
            .arg("-p")
            .arg("#I")
            .output()?;

        if !output.status.success() {
            return Err(TsmError::TmuxCommand(
                "Failed to get current window index".to_string(),
            ));
        }

        let index_str = String::from_utf8(output.stdout).map_err(|_| {
            TsmError::TmuxCommand("Invalid UTF-8 in window index output".to_string())
        })?;

        let index = index_str.trim().parse::<u32>().map_err(|_| {
            TsmError::TmuxCommand(format!(
                "Failed to parse window index: '{}'",
                index_str.trim()
            ))
        })?;

        self.record_access(&session, index)?;
        Ok(())
    }

    pub fn get_last_access(&self, session: &str, window_index: u32) -> Option<u128> {
        let window_id = format!("{}:{}", session, window_index);
        self.entries.get(&window_id).cloned()
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
