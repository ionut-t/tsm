//! A point-in-time description of a session's windows and panes, used to save
//! a running session as a workspace.

use crate::error::{Result, TsmError};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SessionSnapshot {
    pub name: String,
    /// The session's start directory (`#{session_path}`).
    pub path: String,
    pub windows: Vec<WindowSnapshot>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WindowSnapshot {
    pub index: u32,
    /// `None` when tmux named the window automatically after its running
    /// process, which isn't worth saving.
    pub name: Option<String>,
    pub active: bool,
    /// The `#{window_layout}` string.
    pub layout: String,
    pub panes: Vec<PaneSnapshot>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PaneSnapshot {
    pub id: String,
    pub active: bool,
    /// Name of the foreground process, e.g. `zsh` or `nvim`.
    pub current_command: String,
    /// Full command line of the foreground process, when it could be found.
    pub foreground_args: Option<String>,
    pub path: String,
    pub tty: String,
}

/// `list-panes -s -F` format: tab-separated, with the window name last so a
/// tab inside it stays part of the name.
pub const PANE_FORMAT: &str = "#{session_path}\t#{window_index}\t#{window_active}\t#{automatic-rename}\t#{window_layout}\t#{pane_id}\t#{pane_active}\t#{pane_current_command}\t#{pane_current_path}\t#{pane_tty}\t#{window_name}";

/// Parse `list-panes -s -F PANE_FORMAT` output, grouping panes by window in
/// the order tmux lists them. `foreground_args` is left empty for the caller
/// to fill in.
pub fn parse_pane_lines(session: &str, stdout: &str) -> Result<SessionSnapshot> {
    let mut snapshot = SessionSnapshot {
        name: session.to_string(),
        path: String::new(),
        windows: Vec::new(),
    };

    for line in stdout.lines().filter(|l| !l.is_empty()) {
        let fields: Vec<&str> = line.splitn(11, '\t').collect();
        let [
            session_path,
            window_index,
            window_active,
            auto_rename,
            layout,
            pane_id,
            pane_active,
            current_command,
            path,
            tty,
            window_name,
        ] = fields[..]
        else {
            return Err(TsmError::TmuxCommand(format!(
                "unexpected list-panes output: {line}"
            )));
        };

        let index = window_index.parse::<u32>().map_err(|_| {
            TsmError::TmuxCommand(format!("Failed to parse window index: {window_index}"))
        })?;

        snapshot.path = session_path.to_string();

        if snapshot.windows.last().is_none_or(|w| w.index != index) {
            snapshot.windows.push(WindowSnapshot {
                index,
                name: (auto_rename != "1").then(|| window_name.to_string()),
                active: window_active == "1",
                layout: layout.to_string(),
                panes: Vec::new(),
            });
        }

        let window = snapshot.windows.last_mut().expect("pushed above");
        window.panes.push(PaneSnapshot {
            id: pane_id.to_string(),
            active: pane_active == "1",
            current_command: current_command.to_string(),
            foreground_args: None,
            path: path.to_string(),
            tty: tty.to_string(),
        });
    }

    Ok(snapshot)
}

/// Pick the foreground process's command line from `ps -o stat=,args=`
/// output for a pane's tty.
///
/// Several processes can be in the foreground (a pipeline, or an editor's
/// child processes), so this takes the first one whose program name matches
/// tmux's `pane_current_command`. The program's directory is dropped, so
/// `/opt/homebrew/bin/nvim .` comes back as `nvim .`.
pub fn foreground_args(ps_stdout: &str, current_command: &str) -> Option<String> {
    ps_stdout.lines().find_map(|line| {
        let (stat, args) = line.trim().split_once(char::is_whitespace)?;
        let args = args.trim();
        let argv0 = args.split_whitespace().next()?;
        let program = argv0.rsplit('/').next()?.trim_start_matches('-');
        let rest = &args[argv0.len()..];
        (stat.contains('+') && program == current_command).then(|| format!("{program}{rest}"))
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn line(fields: &[&str]) -> String {
        fields.join("\t")
    }

    #[test]
    fn groups_panes_by_window() {
        let stdout = [
            line(&[
                "/code",
                "0",
                "0",
                "1",
                "L0",
                "%0",
                "0",
                "zsh",
                "/code",
                "/dev/ttys1",
                "zsh",
            ]),
            line(&[
                "/code",
                "0",
                "0",
                "1",
                "L0",
                "%2",
                "1",
                "nvim",
                "/code/src",
                "/dev/ttys2",
                "zsh",
            ]),
            line(&[
                "/code",
                "1",
                "1",
                "0",
                "L1",
                "%3",
                "1",
                "zsh",
                "/code",
                "/dev/ttys3",
                "my\twin",
            ]),
        ]
        .join("\n");

        let snapshot = parse_pane_lines("proj", &stdout).unwrap();
        assert_eq!(snapshot.name, "proj");
        assert_eq!(snapshot.path, "/code");
        assert_eq!(snapshot.windows.len(), 2);

        let first = &snapshot.windows[0];
        assert_eq!(first.name, None, "automatic names are dropped");
        assert!(!first.active);
        assert_eq!(first.layout, "L0");
        let ids: Vec<_> = first.panes.iter().map(|p| p.id.as_str()).collect();
        assert_eq!(ids, ["%0", "%2"]);
        assert!(first.panes[1].active);
        assert_eq!(first.panes[1].current_command, "nvim");
        assert_eq!(first.panes[1].path, "/code/src");
        assert_eq!(first.panes[1].tty, "/dev/ttys2");

        let second = &snapshot.windows[1];
        assert_eq!(second.name.as_deref(), Some("my\twin"));
        assert!(second.active);
    }

    #[test]
    fn rejects_lines_with_missing_fields() {
        assert!(parse_pane_lines("p", "/code\t0\t1").is_err());
        assert!(
            parse_pane_lines(
                "p",
                &line(&["/c", "x", "0", "1", "L", "%0", "1", "zsh", "/c", "t", "w"])
            )
            .is_err()
        );
    }

    #[test]
    fn empty_output_has_no_windows() {
        assert!(parse_pane_lines("p", "").unwrap().windows.is_empty());
    }

    #[test]
    fn foreground_args_matches_the_current_command() {
        // Captured from `sleep 300 | cat` in a pane.
        let ps = "Ss   -zsh\nS+   sleep 300\nS+   cat\n";
        assert_eq!(foreground_args(ps, "sleep").as_deref(), Some("sleep 300"));
        assert_eq!(foreground_args(ps, "cat").as_deref(), Some("cat"));
    }

    #[test]
    fn foreground_args_ignores_background_processes_and_paths() {
        let ps = "S    nvim old.txt\nS+   /opt/homebrew/bin/nvim .\n";
        assert_eq!(foreground_args(ps, "nvim").as_deref(), Some("nvim ."));
        assert_eq!(foreground_args(ps, "vim"), None);
        assert_eq!(foreground_args("", "nvim"), None);
    }
}
