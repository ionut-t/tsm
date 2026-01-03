use std::collections::HashMap;
use std::fs::File;
use std::io::{BufRead, BufReader, Write};
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

use crate::error::Result;
use crate::tmux::TmuxClient;

pub struct WindowHistory {
    file_path: PathBuf,
    entries: HashMap<String, u64>,
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
                    && let Ok(timestamp) = parts[1].parse::<u64>()
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

        let mut file = File::create(&self.file_path)?;
        for (window_id, timestamp) in entries {
            writeln!(file, "{}\t{}", window_id, timestamp)?;
        }
        Ok(())
    }

    pub fn record_access(&mut self, session: &str, window_index: u32) {
        let window_id = format!("{}:{}", session, window_index);
        let timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs();
        self.entries.insert(window_id, timestamp);
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

        if let Ok(index_str) = String::from_utf8(output.stdout)
            && let Ok(index) = index_str.trim().parse::<u32>()
        {
            self.record_access(&session, index);
        }
        Ok(())
    }

    pub fn get_last_access(&self, session: &str, window_index: u32) -> Option<u64> {
        let window_id = format!("{}:{}", session, window_index);
        self.entries.get(&window_id).cloned()
    }
}
