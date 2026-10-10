//! Turn a snapshot of a running tmux session into workspace TOML.
//!
//! The output is a starting point for the user to edit, not an exact copy:
//! commands are written commented out so launching the workspace never
//! re-runs something unexpected, and row heights are left out. Any tmux
//! layout fits, because a pane can hold rows of its own.

use std::fmt::Write as _;
use std::path::Path;

use crate::tmux::snapshot::{PaneSnapshot, SessionSnapshot, WindowSnapshot};
use crate::workspace::layout::{self, PaneContent, PaneShape, RowShape};
use crate::zoxide::abbreviate_home;

/// Programs treated as an idle shell, so the pane is saved without a command.
const SHELLS: &[&str] = &[
    "sh", "bash", "zsh", "fish", "dash", "ksh", "tcsh", "csh", "nu", "xonsh", "elvish",
];

pub struct Capture {
    pub toml: String,
    /// Things the user should know were not saved faithfully.
    pub warnings: Vec<String>,
}

/// Render `snapshot` as a workspace named `name`. `home` is used to shorten
/// paths to `~/...`.
pub fn render(name: &str, snapshot: &SessionSnapshot, home: Option<&str>) -> Capture {
    let mut warnings = Vec::new();
    let mut out = String::new();

    // tmux reports pane paths with symlinks resolved (e.g. `/private/tmp` for
    // `/tmp` on macOS), so compare against the resolved root.
    let root = std::fs::canonicalize(&snapshot.path)
        .map(|p| p.to_string_lossy().into_owned())
        .unwrap_or_else(|_| snapshot.path.clone());

    let _ = writeln!(
        out,
        "# Saved from tmux session {}.\n\
         # Commands are commented out: uncomment the ones to run on launch.",
        quote(&snapshot.name)
    );
    let _ = writeln!(out, "name = {}", quote(name));
    let _ = writeln!(
        out,
        "root = {}",
        quote(&abbreviate_home(&snapshot.path, home))
    );

    for window in &snapshot.windows {
        out.push('\n');
        render_window(&mut out, &mut warnings, window, &root, home);
    }

    Capture {
        toml: out,
        warnings,
    }
}

fn render_window(
    out: &mut String,
    warnings: &mut Vec<String>,
    window: &WindowSnapshot,
    root: &str,
    home: Option<&str>,
) {
    let _ = writeln!(out, "[[window]]");
    if let Some(name) = &window.name {
        let _ = writeln!(out, "name = {}", quote(name));
    }
    if window.active {
        let _ = writeln!(out, "focus = true");
    }

    let rows = match layout::parse(&window.layout) {
        Ok(cell) => layout::to_rows(&cell),
        Err(_) => {
            let label = match &window.name {
                Some(name) => format!("window {} ({name})", window.index),
                None => format!("window {}", window.index),
            };
            warnings.push(format!(
                "{label}: couldn't read its tmux layout; saved its panes side by side"
            ));
            vec![flattened_row(window)]
        }
    };

    let ctx = PaneContext { window, root, home };
    write_rows(out, &ctx, &rows, "window");
}

/// What every pane in a window needs to describe itself.
struct PaneContext<'a> {
    window: &'a WindowSnapshot,
    root: &'a str,
    home: Option<&'a str>,
}

/// Write rows as `[[<parent>.row]]` tables, recursing into panes that hold
/// rows of their own (`[[window.row.pane.row]]` and so on).
fn write_rows(out: &mut String, ctx: &PaneContext, rows: &[RowShape], parent: &str) {
    let row_table = format!("{parent}.row");
    for row in rows {
        let _ = writeln!(out, "\n[[{row_table}]]");
        for shape in &row.panes {
            write_pane(out, ctx, shape, &row_table);
        }
    }
}

fn write_pane(out: &mut String, ctx: &PaneContext, shape: &PaneShape, row_table: &str) {
    let pane_table = format!("{row_table}.pane");
    let _ = writeln!(out, "\n[[{pane_table}]]");
    if let Some(width) = shape.width {
        let _ = writeln!(out, "width = {width}");
    }

    let pane_id = match &shape.content {
        PaneContent::Rows(rows) => return write_rows(out, ctx, rows, &pane_table),
        PaneContent::Leaf(pane_id) => pane_id,
    };
    // tmux reported this pane in the layout, so it's in the pane list too;
    // if not, the empty pane still keeps its place.
    let Some(pane) = ctx.window.panes.iter().find(|p| &p.id == pane_id) else {
        return;
    };
    if let Some(command) = pane_command(pane) {
        let _ = writeln!(out, "# command = {}", quote(&command));
    }
    if pane.path != ctx.root {
        let _ = writeln!(out, "# was in: {}", abbreviate_home(&pane.path, ctx.home));
    }
    if ctx.window.active && pane.active {
        let _ = writeln!(out, "focus = true");
    }
}

/// Every pane in one row, in the order tmux listed them. Only used when the
/// window's layout string can't be read; launch splits the row evenly.
fn flattened_row(window: &WindowSnapshot) -> RowShape {
    RowShape {
        panes: window
            .panes
            .iter()
            .map(|pane| PaneShape {
                width: None,
                content: PaneContent::Leaf(pane.id.clone()),
            })
            .collect(),
    }
}

/// The command a pane is running, or `None` for an idle shell.
fn pane_command(pane: &PaneSnapshot) -> Option<String> {
    let program = Path::new(&pane.current_command)
        .file_name()
        .map(|n| n.to_string_lossy().trim_start_matches('-').to_string())
        .unwrap_or_default();
    if program.is_empty() || SHELLS.contains(&program.as_str()) {
        return None;
    }
    Some(
        pane.foreground_args
            .clone()
            .unwrap_or_else(|| pane.current_command.clone()),
    )
}

/// A TOML basic string, escaped.
fn quote(s: &str) -> String {
    toml::Value::String(s.to_string()).to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::workspace::config::Workspace;

    fn pane(id: &str, command: &str, path: &str) -> PaneSnapshot {
        PaneSnapshot {
            id: id.to_string(),
            active: false,
            current_command: command.to_string(),
            foreground_args: None,
            path: path.to_string(),
            tty: String::new(),
        }
    }

    fn window(
        index: u32,
        name: Option<&str>,
        layout: &str,
        panes: Vec<PaneSnapshot>,
    ) -> WindowSnapshot {
        WindowSnapshot {
            index,
            name: name.map(str::to_string),
            active: false,
            layout: layout.to_string(),
            panes,
        }
    }

    fn snapshot(windows: Vec<WindowSnapshot>) -> SessionSnapshot {
        SessionSnapshot {
            name: "proj".to_string(),
            path: "/home/me/code/proj".to_string(),
            windows,
        }
    }

    /// Render and parse back, so every test also checks the TOML is valid.
    fn render_parsed(snapshot: &SessionSnapshot) -> (Capture, Workspace) {
        let capture = render("saved", snapshot, Some("/home/me"));
        let workspace: Workspace = toml::from_str(&capture.toml)
            .unwrap_or_else(|e| panic!("invalid TOML ({e}):\n{}", capture.toml));
        (capture, workspace)
    }

    #[test]
    fn saves_rows_sizes_names_and_focus() {
        let mut editor = pane("%0", "nvim", "/home/me/code/proj");
        editor.foreground_args = Some("nvim .".to_string());
        editor.active = true;
        let mut code = window(
            0,
            Some("code"),
            "f26b,120x40,0,0[120x27,0,0,0,120x12,0,28{60x12,0,28,1,59x12,61,28,2}]",
            vec![
                editor,
                pane("%1", "zsh", "/home/me/code/proj"),
                pane("%2", "cargo", "/home/me/code/proj"),
            ],
        );
        code.active = true;
        let shell = window(
            1,
            None,
            "55af,120x40,0,0,3",
            vec![pane("%3", "-zsh", "/home/me/code/proj")],
        );

        let (capture, ws) = render_parsed(&snapshot(vec![code, shell]));
        assert!(capture.warnings.is_empty());
        assert_eq!(ws.name, "saved");
        assert_eq!(ws.root.as_deref(), Some("~/code/proj"));
        assert_eq!(ws.window.len(), 2);

        let code = &ws.window[0];
        assert_eq!(code.name.as_deref(), Some("code"));
        assert!(code.focus);
        assert_eq!(code.row.len(), 2);
        // Row heights are left to tmux.
        assert!(code.row.iter().all(|r| r.height.is_none()));
        assert!(!capture.toml.contains("height"));
        assert!(code.row[0].pane[0].focus);
        assert_eq!(code.row[1].pane.len(), 2);
        // An even split needs no widths: launch splits evenly by default.
        assert!(code.row[1].pane.iter().all(|p| p.width.is_none()));

        let shell = &ws.window[1];
        assert_eq!(shell.name, None);
        assert!(!shell.focus);
    }

    #[test]
    fn commands_are_commented_out_and_shells_are_skipped() {
        let mut editor = pane("%0", "nvim", "/home/me/code/proj");
        editor.foreground_args = Some("nvim .".to_string());
        let (capture, ws) = render_parsed(&snapshot(vec![
            window(0, None, "55af,120x40,0,0,0", vec![editor]),
            window(
                1,
                None,
                "55af,120x40,0,0,1",
                vec![pane("%1", "-zsh", "/home/me/code/proj")],
            ),
            window(
                2,
                None,
                "55af,120x40,0,0,2",
                vec![pane("%2", "htop", "/home/me/code/proj")],
            ),
        ]));

        assert!(capture.toml.contains("# command = \"nvim .\""));
        // Without `ps` output, the bare process name is the best guess.
        assert!(capture.toml.contains("# command = \"htop\""));
        assert!(!capture.toml.contains("zsh"));
        assert!(
            ws.window
                .iter()
                .flat_map(|w| &w.row)
                .flat_map(|r| &r.pane)
                .all(|p| p.command.is_none())
        );
    }

    #[test]
    fn notes_panes_outside_the_root() {
        let (capture, _) = render_parsed(&snapshot(vec![window(
            0,
            None,
            "791e,120x40,0,0{60x40,0,0,0,59x40,61,0,1}",
            vec![
                pane("%0", "zsh", "/home/me/code/proj"),
                pane("%1", "zsh", "/home/me/code/proj/src"),
            ],
        )]));
        assert_eq!(capture.toml.matches("# was in:").count(), 1);
        assert!(capture.toml.contains("# was in: ~/code/proj/src"));
    }

    #[test]
    fn any_layout_is_saved_as_nested_rows() {
        // A real layout: one tall pane on the left, the right half split
        // into a top pane and two bottom panes. tmux lists panes in its own
        // order; the file follows the layout.
        const BIG_LEFT: &str = "bcd7,152x39,0,0{76x39,0,0,88,75x39,77,0[75x19,77,0,89,75x19,77,20{37x19,77,20,90,37x19,115,20,91}]}";
        let (capture, ws) = render_parsed(&snapshot(vec![window(
            0,
            Some("dev"),
            BIG_LEFT,
            vec![
                pane("%91", "four", "/home/me/code/proj"),
                pane("%88", "one", "/home/me/code/proj"),
                pane("%89", "two", "/home/me/code/proj"),
                pane("%90", "three", "/home/me/code/proj"),
            ],
        )]));

        assert!(capture.warnings.is_empty());
        assert!(capture.toml.contains("[[window.row.pane.row.pane]]"));
        assert!(!capture.toml.contains("width"));

        let window = &ws.window[0];
        assert_eq!(window.row.len(), 1);
        let [left, right] = &window.row[0].pane[..] else {
            panic!("expected two panes side by side");
        };
        assert!(left.row.is_empty());
        assert_eq!(right.row.len(), 2);
        assert_eq!(right.row[0].pane.len(), 1);
        assert_eq!(right.row[1].pane.len(), 2);

        let at = |cmd: &str| {
            capture
                .toml
                .find(&format!("# command = \"{cmd}\""))
                .unwrap()
        };
        assert!(at("one") < at("two") && at("two") < at("three") && at("three") < at("four"));
    }

    #[test]
    fn saved_nested_rows_relaunch_as_the_same_layout() {
        // Save a layout, then build the launch layout from the saved file at
        // the same size: the geometry must come back unchanged.
        const NESTED: &str = "5c54,120x40,0,0[120x20,0,0,6,120x19,0,21{60x19,0,21,7,59x19,61,21[59x9,61,21,8,59x9,61,31,9]}]";
        let panes = (6..=9)
            .map(|i| pane(&format!("%{i}"), "zsh", "/home/me/code/proj"))
            .collect();
        let (_, ws) = render_parsed(&snapshot(vec![window(0, None, NESTED, panes)]));

        let relaunched =
            layout::render(&crate::workspace::arrange::layout(&ws.window[0], 120, 40).unwrap());
        // Same geometry as NESTED; only the pane ids differ.
        assert_eq!(
            relaunched.split_once(',').unwrap().1,
            "120x40,0,0[120x20,0,0,0,120x19,0,21{60x19,0,21,1,59x19,61,21[59x9,61,21,2,59x9,61,31,3]}]"
        );
    }

    #[test]
    fn unparseable_layouts_fall_back_to_tmux_pane_order() {
        let (capture, ws) = render_parsed(&snapshot(vec![window(
            0,
            None,
            "garbage",
            vec![pane("%0", "zsh", "/x"), pane("%1", "zsh", "/x")],
        )]));
        assert_eq!(capture.warnings.len(), 1);
        assert_eq!(ws.window[0].row[0].pane.len(), 2);
        assert!(ws.window[0].row[0].pane.iter().all(|p| p.width.is_none()));
    }

    #[test]
    fn names_with_quotes_are_escaped() {
        let (_, ws) = render_parsed(&snapshot(vec![window(
            0,
            Some(r#"say "hi" \o/"#),
            "55af,120x40,0,0,0",
            vec![pane("%0", "zsh", "/x")],
        )]));
        assert_eq!(ws.window[0].name.as_deref(), Some(r#"say "hi" \o/"#));
    }
}
