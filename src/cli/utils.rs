use crate::error::Result;
use crate::history::HistoryStore;
use crate::tmux::{Tmux, Window};

pub const PREVIEW_CMD: &str = r#"
# Sanitize input to prevent command injection
PANE_ID=$(echo {} | cut -f1 | sed 's/[^a-zA-Z0-9_%@:-]//g')
# Validate that PANE_ID looks like a tmux pane ID (e.g., %1, %2, etc.)
if [[ "$PANE_ID" =~ ^%[0-9]+$ ]]; then
    tmux capture-pane -e -p -t "$PANE_ID" 2>/dev/null || echo "No preview available"
else
    echo "Invalid pane ID"
fi
"#;

pub const PREVIEW_LS_TREE_CMD: &str = r#"
dir={}; dir="${dir/#\~/$HOME}"; command -v eza >/dev/null && eza --color=always --icons=always --tree --level=1 --group-directories-first "$dir" || ls "$dir"
"#;

/// POSIX-shell-quote a string for safe interpolation into an fzf `--preview`
/// command (fzf runs these through `sh -c`).
///
/// Wraps the value in single quotes and escapes any embedded single quote as
/// `'\''`, so paths containing spaces or shell metacharacters can't break out
/// of or alter the command. fzf already quotes its own `{}`/`{n}` placeholders;
/// only the segments we splice in need this.
///
/// Deliberately hand-rolled rather than pulling in a crate (e.g. `shell-escape`):
/// single-quote quoting is a complete, unambiguous algorithm — the only byte
/// that can't appear literally inside `'…'` is `'` itself, handled here — so
/// there's no edge-case tail for a dependency to cover. fzf always uses `sh -c`,
/// so POSIX rules are exactly right; we don't need a crate's cross-shell
/// (cmd.exe/PowerShell) handling, and this stays smaller and more auditable than
/// a new supply-chain + compile-time dependency for three lines.
pub fn shell_quote(s: &str) -> String {
    format!("'{}'", s.replace('\'', "'\\''"))
}

/// Sort windows by access time (most recent first) and return indexed list
pub fn sort_windows_by_history(
    windows: Vec<Window>,
    history: &dyn HistoryStore,
) -> Vec<(Window, u128)> {
    let mut indexed_windows: Vec<_> = windows
        .into_iter()
        .map(|w| {
            let last_access = history.last_access(&w.session_name, w.index).unwrap_or(0);
            (w, last_access)
        })
        .collect();
    indexed_windows.sort_by_key(|w| std::cmp::Reverse(w.1));
    indexed_windows
}

/// Record the current window's access, if we're inside tmux.
///
/// Lives here (the command layer) rather than on the history store so the
/// persistence layer stays free of any tmux dependency.
pub fn record_current_window(client: &dyn Tmux, history: &mut dyn HistoryStore) -> Result<()> {
    if client.is_inside_tmux() {
        let (session, index) = client.get_current_window()?;
        history.record(&session, index)?;
    }
    Ok(())
}

/// Record window access and switch to it
pub fn switch_to_window(
    client: &dyn Tmux,
    window: &Window,
    history: &mut dyn HistoryStore,
) -> Result<()> {
    history.record(&window.session_name, window.index)?;

    if client.is_inside_tmux() {
        client.switch_to_window(&window.session_name, window.index)?;
    } else {
        client.attach_to_window(&window.session_name, window.index)?;
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::InMemoryHistory;

    #[test]
    fn shell_quote_wraps_plain_strings() {
        assert_eq!(shell_quote("/home/user/.config"), "'/home/user/.config'");
    }

    #[test]
    fn shell_quote_preserves_spaces_and_metacharacters() {
        assert_eq!(
            shell_quote("/tmp/my configs; rm -rf /"),
            "'/tmp/my configs; rm -rf /'"
        );
    }

    #[test]
    fn shell_quote_escapes_embedded_single_quotes() {
        // A single quote must close, escape, and reopen: `'\''`.
        assert_eq!(shell_quote("it's"), r#"'it'\''s'"#);
    }

    fn window(session: &str, index: u32) -> Window {
        Window {
            session_name: session.to_string(),
            index,
            name: format!("win{}", index),
            pane_id: format!("%{}", index),
        }
    }

    #[test]
    fn sorts_most_recently_accessed_first() {
        let history = InMemoryHistory::seeded(&[("a", 0, 100), ("b", 1, 300), ("c", 2, 200)]);

        let windows = vec![window("a", 0), window("b", 1), window("c", 2)];
        let sorted = sort_windows_by_history(windows, &history);

        let order: Vec<_> = sorted
            .iter()
            .map(|(w, _)| (w.session_name.clone(), w.index))
            .collect();
        // Descending by timestamp: b(300), c(200), a(100).
        assert_eq!(
            order,
            vec![
                ("b".to_string(), 1),
                ("c".to_string(), 2),
                ("a".to_string(), 0),
            ]
        );
    }

    #[test]
    fn windows_without_history_sort_last_with_zero_access() {
        let history = InMemoryHistory::seeded(&[("known", 0, 500)]);

        let windows = vec![window("unknown", 5), window("known", 0)];
        let sorted = sort_windows_by_history(windows, &history);

        assert_eq!(sorted[0].0.session_name, "known");
        assert_eq!(sorted[0].1, 500);
        assert_eq!(sorted[1].0.session_name, "unknown");
        assert_eq!(sorted[1].1, 0, "missing history yields a zero timestamp");
    }

    #[test]
    fn empty_input_yields_empty_output() {
        let history = InMemoryHistory::new();
        assert!(sort_windows_by_history(vec![], &history).is_empty());
    }
}
