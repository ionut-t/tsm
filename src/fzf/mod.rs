use std::io::Write;
use std::process::{Command, Stdio};

use crate::error::Result;

pub struct FzfPicker {
    prompt: String,
    preview_command: Option<String>,
    preview_window: String,
    delimiter: Option<String>,
    with_nth: Option<String>,
    nth: Option<String>,
    border: Option<String>,
    border_label: Option<String>,
    preview_label: Option<String>,
    margin: Option<String>,
    padding: Option<String>,
    header: Option<String>,
    no_hscroll: bool,
}

impl FzfPicker {
    pub fn new() -> Self {
        Self {
            prompt: "Select an item: ".to_string(),
            preview_command: None,
            preview_window: "right:60%".to_string(),
            delimiter: None,
            with_nth: None,
            nth: None,
            border: None,
            border_label: None,
            preview_label: None,
            margin: None,
            padding: None,
            header: None,
            no_hscroll: false,
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

    /// Overrides the preview window position/size (fzf's `--preview-window`).
    pub fn with_preview_window(mut self, preview_window: &str) -> Self {
        self.preview_window = preview_window.to_string();
        self
    }

    /// Draws a border around the finder (fzf's `--border`, e.g. `rounded`).
    pub fn with_border(mut self, style: &str) -> Self {
        self.border = Some(style.to_string());
        self
    }

    /// Sets a label on the finder border (fzf's `--border-label`).
    pub fn with_border_label(mut self, label: &str) -> Self {
        self.border_label = Some(label.to_string());
        self
    }

    /// Sets a label on the preview window border (fzf's `--preview-label`).
    pub fn with_preview_label(mut self, label: &str) -> Self {
        self.preview_label = Some(label.to_string());
        self
    }

    /// Sets the margin around the finder (fzf's `--margin`).
    pub fn with_margin(mut self, margin: &str) -> Self {
        self.margin = Some(margin.to_string());
        self
    }

    /// Sets the padding inside the finder border (fzf's `--padding`).
    pub fn with_padding(mut self, padding: &str) -> Self {
        self.padding = Some(padding.to_string());
        self
    }

    /// Sets a sticky header line shown above the list (fzf's `--header`).
    pub fn with_header(mut self, header: &str) -> Self {
        self.header = Some(header.to_string());
        self
    }

    /// Disables horizontal scrolling of matched rows (fzf's `--no-hscroll`), so
    /// column layouts stay put instead of shifting to reveal the match.
    pub fn no_hscroll(mut self) -> Self {
        self.no_hscroll = true;
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

    /// Restricts the fields fzf searches against (fzf's `--nth`).
    pub fn with_search_nth(mut self, nth: &str) -> Self {
        self.nth = Some(nth.to_string());
        self
    }

    pub fn pick(&self, items: &[String]) -> Result<Option<String>> {
        let mut fzf = Command::new("fzf");
        fzf.arg("--ansi").arg(format!("--prompt={}", self.prompt));

        if let Some(border) = &self.border {
            fzf.arg(format!("--border={}", border));
        }

        if let Some(label) = &self.border_label {
            fzf.arg(format!("--border-label={}", label));
        }

        if let Some(margin) = &self.margin {
            fzf.arg(format!("--margin={}", margin));
        }

        if let Some(padding) = &self.padding {
            fzf.arg(format!("--padding={}", padding));
        }

        if let Some(header) = &self.header {
            fzf.arg(format!("--header={}", header));
        }

        if self.no_hscroll {
            fzf.arg("--no-hscroll");
        }

        if let Some(delimiter) = &self.delimiter {
            fzf.arg("--delimiter").arg(delimiter);
        }

        if let Some(nth) = &self.with_nth {
            fzf.arg("--with-nth").arg(nth);
        }

        if let Some(nth) = &self.nth {
            fzf.arg("--nth").arg(nth);
        }

        if let Some(preview_cmd) = &self.preview_command {
            fzf.arg("--preview")
                .arg(preview_cmd)
                .arg("--preview-window")
                .arg(&self.preview_window);

            if let Some(label) = &self.preview_label {
                fzf.arg(format!("--preview-label={}", label));
            }
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
