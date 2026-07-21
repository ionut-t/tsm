use std::io::Write;
use std::process::{Command, Stdio};

use crate::error::Result;

/// Display configuration for a single fuzzy-finder selection.
///
/// Built with the `with_*` methods and handed to [`Picker::pick`]. Kept separate
/// from the [`Picker`] implementation so commands can be tested against an
/// in-memory picker without spawning `fzf`.
pub struct PickerOptions {
    prompt: String,
    preview_command: Option<String>,
    preview_window: String,
    delimiter: Option<String>,
    with_nth: Option<String>,
    nth: Option<String>,
    border: Option<String>,
    border_label: Option<String>,
    preview_label: Option<String>,
    header: Option<String>,
    no_hscroll: bool,
}

impl Default for PickerOptions {
    fn default() -> Self {
        Self::new()
    }
}

impl PickerOptions {
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
}

/// Presents a list of items and returns the user's choice.
///
/// The real implementation ([`FzfPicker`]) shells out to `fzf`; tests use an
/// in-memory double.
pub trait Picker {
    /// Present `items` configured by `options`, returning the selected line, or
    /// `None` if the user cancelled the selection.
    fn pick(&self, options: &PickerOptions, items: &[String]) -> Result<Option<String>>;
}

/// [`Picker`] backed by the `fzf` binary.
#[derive(Default)]
pub struct FzfPicker;

impl FzfPicker {
    pub fn new() -> Self {
        FzfPicker
    }
}

impl Picker for FzfPicker {
    fn pick(&self, options: &PickerOptions, items: &[String]) -> Result<Option<String>> {
        let mut fzf = Command::new("fzf");
        fzf.arg("--ansi")
            .arg(format!("--prompt={}", options.prompt));

        if let Some(border) = &options.border {
            fzf.arg(format!("--border={}", border));
        }

        if let Some(label) = &options.border_label {
            fzf.arg(format!("--border-label={}", label));
        }

        if let Some(header) = &options.header {
            fzf.arg(format!("--header={}", header));
        }

        if options.no_hscroll {
            fzf.arg("--no-hscroll");
        }

        if let Some(delimiter) = &options.delimiter {
            fzf.arg("--delimiter").arg(delimiter);
        }

        if let Some(nth) = &options.with_nth {
            fzf.arg("--with-nth").arg(nth);
        }

        if let Some(nth) = &options.nth {
            fzf.arg("--nth").arg(nth);
        }

        if let Some(preview_cmd) = &options.preview_command {
            fzf.arg("--preview")
                .arg(preview_cmd)
                .arg("--preview-window")
                .arg(&options.preview_window);

            if let Some(label) = &options.preview_label {
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
