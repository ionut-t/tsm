use crate::error::{Result, TsmError};
use std::process::Command;

/// Supplies a list of candidate directories for the "new session" picker.
///
/// Abstracted behind a trait so `new`'s directory-selection flow can be tested
/// without invoking the real `zoxide` binary.
pub trait DirectorySource {
    fn query_directories(&self) -> Result<Vec<String>>;
}

/// [`DirectorySource`] backed by the `zoxide` binary (`zoxide query -l`).
#[derive(Default)]
pub struct Zoxide;

impl Zoxide {
    pub fn new() -> Self {
        Zoxide
    }
}

impl DirectorySource for Zoxide {
    fn query_directories(&self) -> Result<Vec<String>> {
        match Command::new("zoxide").arg("query").arg("-l").output() {
            Ok(output) => {
                let home = std::env::home_dir().map(|h| h.to_string_lossy().into_owned());
                let stdout = String::from_utf8_lossy(&output.stdout);
                let dirs = stdout
                    .lines()
                    .map(|line| abbreviate_home(line, home.as_deref()))
                    .collect();
                Ok(dirs)
            }
            Err(_) => Err(TsmError::ZoxideQueryFailed),
        }
    }
}

/// Replace a leading home-directory prefix with `~` for display, matching the
/// convention zoxide paths are shown in the picker.
fn abbreviate_home(line: &str, home: Option<&str>) -> String {
    if let Some(home) = home
        && !home.is_empty()
        && line.starts_with(home)
    {
        return line.replacen(home, "~", 1);
    }
    line.to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn abbreviates_a_leading_home_prefix() {
        assert_eq!(
            abbreviate_home("/home/tester/projects/app", Some("/home/tester")),
            "~/projects/app"
        );
    }

    #[test]
    fn replaces_only_the_first_occurrence() {
        // A home-like segment later in the path is left untouched.
        assert_eq!(
            abbreviate_home("/home/tester/home/tester", Some("/home/tester")),
            "~/home/tester"
        );
    }

    #[test]
    fn leaves_non_home_paths_unchanged() {
        assert_eq!(
            abbreviate_home("/etc/nginx", Some("/home/tester")),
            "/etc/nginx"
        );
    }

    #[test]
    fn leaves_path_unchanged_when_home_is_unknown() {
        assert_eq!(abbreviate_home("/home/tester/x", None), "/home/tester/x");
    }

    #[test]
    fn empty_home_does_not_prepend_tilde() {
        // Guards the degenerate case where home would match every line.
        assert_eq!(abbreviate_home("/anything", Some("")), "/anything");
    }
}
