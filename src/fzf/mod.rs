use std::io::Write;
use std::process::{Command, Stdio};

use crate::error::Result;

pub struct FzfPicker {
    prompt: String,
    preview_command: Option<String>,
    preview_window: String,
    delimiter: Option<String>,
    with_nth: Option<String>,
}

impl FzfPicker {
    pub fn new() -> Self {
        Self {
            prompt: "Select an item: ".to_string(),
            preview_command: None,
            preview_window: "right:60%".to_string(),
            delimiter: None,
            with_nth: None,
        }
    }

    pub fn with_prompt(mut self, prompt: &str) -> Self {
        self.prompt = prompt.to_string();
        self
    }

    pub fn with_preview_command(mut self, command: &str) -> Self {
        self.preview_command = Some(command.to_string());
        self
    }

    pub fn with_delimiter(mut self, delimiter: &str) -> Self {
        self.delimiter = Some(delimiter.to_string());
        self
    }

    pub fn with_nth(mut self, nth: &str) -> Self {
        self.with_nth = Some(nth.to_string());
        self
    }

    pub fn pick(&self, items: &[String]) -> Result<Option<String>> {
        let mut fzf = Command::new("fzf");
        fzf.arg("--ansi").arg(format!("--prompt={}", self.prompt));

        if let Some(delimiter) = &self.delimiter {
            fzf.arg("--delimiter").arg(delimiter);
        }

        if let Some(nth) = &self.with_nth {
            fzf.arg("--with-nth").arg(nth);
        }

        if let Some(preview_cmd) = &self.preview_command {
            fzf.arg("--preview")
                .arg(preview_cmd)
                .arg("--preview-window")
                .arg(&self.preview_window);
        }

        let mut child = fzf.stdin(Stdio::piped()).stdout(Stdio::piped()).spawn()?;

        {
            let stdin = child.stdin.as_mut().ok_or_else(|| {
                crate::error::TsmError::Fzf("Failed to open fzf stdin".to_string())
            })?;

            for item in items {
                writeln!(stdin, "{}", item)?;
            }
        }

        let output = child.wait_with_output()?;

        if output.status.success() {
            let selection = String::from_utf8_lossy(&output.stdout).trim().to_string();
            Ok(Some(selection))
        } else {
            Ok(None)
        }
    }
}
