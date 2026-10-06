use std::collections::HashSet;
use std::env;
use std::path::PathBuf;
use std::process::Command;

use clap::CommandFactory;

use crate::cli::help_docs::{Docs, Example};
use crate::cli::utils::shell_quote;
use crate::error::Result;
use crate::fzf::{Picker, PickerOptions};
use crate::tmux::{KeyBinding, Tmux};

use super::commands::Cli;

// ANSI styling for the picker rows (fzf is launched with `--ansi`).
const ALIAS_COLOR: &str = "\x1b[35m"; // magenta — the keybinding-analog column
const TSM_COLOR: &str = "\x1b[36m"; // cyan — tsm command names
const TMUX_COLOR: &str = "\x1b[32m"; // green — tmux command names
const KEY_COLOR: &str = "\x1b[33m"; // yellow — the user's own key bindings
const DESC_COLOR: &str = "\x1b[2m"; // dim — descriptions
const RESET: &str = "\x1b[0m";

// Styling for the rendered tldr-style doc (preview pane + final output).
const BOLD: &str = "\x1b[1m";
const TITLE_COLOR: &str = "\x1b[1;36m"; // bold cyan
const COMMENT_COLOR: &str = "\x1b[2m"; // dim — example descriptions
const CMD_COLOR: &str = "\x1b[32m"; // green — example commands

/// A single command entry, before it is laid out into aligned columns.
struct Entry {
    /// `"tsm"`, `"tmux"` or `"key"` — the hidden key that routes help/preview.
    kind: &'static str,
    /// Hidden id passed back to `--render`: the command name for `tsm`/`tmux`,
    /// `"<table> <key>"` for a key binding.
    id: String,
    /// Command name shown in the list.
    name: String,
    /// Short alias (Helix-style keybinding column); empty if none.
    alias: String,
    /// One-line human description, shown in the list and searched; empty if
    /// none (a key binding without a `bind -N` note).
    description: String,
    /// Shown in place of an empty `description` but never searched — for a key
    /// binding, its raw command, whose flags and `#{...}` formats would
    /// otherwise match nearly any query.
    detail: String,
}

/// Browse all tsm and tmux commands in a Helix-style fuzzy picker.
///
/// Lists every tsm subcommand, the key bindings your tmux config adds or
/// changes, and every tmux command, each with a short description, and shows
/// tldr-style usage examples in the preview. Selecting an entry prints its doc.
#[derive(clap::Parser, Debug)]
pub struct HelpCommand {
    /// fzf prompt
    #[clap(short = 'P', long, default_value = "Command: ")]
    prompt: String,

    /// Internal: render the tldr doc for a single command (used by the preview).
    #[clap(long, hide = true)]
    render: Option<String>,

    /// Internal: source of the `--render` command (`tsm`, `tmux` or `key`).
    #[clap(long, hide = true, default_value = "tsm")]
    source: String,
}

impl HelpCommand {
    pub fn run(&self, tmux: &dyn Tmux, picker: &dyn Picker) -> Result<()> {
        // Preview / doc-render mode: print one command's doc and exit.
        if let Some(name) = &self.render {
            render_doc(tmux, &self.source, name);
            return Ok(());
        }

        let (prefix, key_entries) = key_entries(tmux);

        let mut entries = tsm_entries();
        entries.extend(key_entries);
        entries.extend(tmux_entries());

        // Column widths so aliases and command names line up vertically.
        let alias_width = entries
            .iter()
            .map(|e| e.alias.chars().count())
            .max()
            .unwrap_or(0);
        let name_width = entries
            .iter()
            .map(|e| e.name.chars().count())
            .max()
            .unwrap_or(0);

        let items = entries
            .iter()
            .map(|e| row(e, alias_width, name_width))
            .collect::<Vec<String>>();

        // Rows render with a one-column tabstop, so the description sits two
        // columns past the padded keys; spaces here line the header up with it.
        let header = format!(
            "{dc}{alias:<aw$} {name:<nw$}  {desc}{r}",
            dc = DESC_COLOR,
            alias = "alias",
            aw = alias_width,
            name = "command",
            nw = name_width,
            desc = "description",
            r = RESET,
        );

        // The preview re-invokes this binary to render the highlighted command's
        // doc. Using the current exe path keeps it working off `PATH`; fzf shell-
        // quotes {1}/{2} (the hidden kind/name fields), so they are injection-safe.
        // The exe path is ours to quote — `shell_quote` handles spaces and any
        // embedded quote (which naive `'{}'` wrapping would break on).
        let exe = env::current_exe()
            .ok()
            .and_then(|p| p.to_str().map(String::from))
            .unwrap_or_else(|| "tsm".to_string());
        let preview_cmd = format!("{} help --source {{1}} --render {{2}}", shell_quote(&exe));

        // Surface the prefix in the frame, since every prefix binding uses it.
        let border_label = match &prefix {
            Some(prefix) => format!(" Commands · prefix {prefix} "),
            None => " Commands ".to_string(),
        };

        let options = PickerOptions::new()
            .with_prompt(&self.prompt)
            .with_delimiter("\t")
            // Show the keys (3), detail (4) and description (5) columns...
            .with_nth("3,4,5")
            // ...but search only keys and description. `--nth` counts fields
            // left after `--with-nth`, so 1 is the alias+command column and 3
            // the description; the raw-command detail stays unsearchable.
            .with_search_nth("1,3")
            .with_tabstop(1)
            .no_hscroll()
            .with_preview_command(&preview_cmd)
            .with_preview_window("right:55%:border-rounded")
            .with_preview_label(" Help ")
            .with_border("rounded")
            .with_border_label(&border_label)
            .with_header(&header);

        let selection = match picker.pick(&options, &items)? {
            Some(sel) => sel,
            None => return Ok(()), // User canceled
        };

        let mut fields = selection.split('\t');
        let kind = fields.next().unwrap_or("");
        let id = fields.next().unwrap_or("");
        if id.is_empty() {
            return Ok(());
        }

        // Print the same tldr doc the preview showed.
        render_doc(tmux, kind, id);
        Ok(())
    }
}

/// Lay out one picker row as five tab-delimited fields:
///   1: kind   — hidden; routes the preview and the final doc render.
///   2: id     — hidden; what `--render` looks up.
///   3: keys   — shown, searched: aligned `alias command` columns.
///   4: detail — shown, NOT searched: ` <detail>` when there's no description.
///   5: desc   — shown, searched: the short human description.
///
/// fzf shows each field's trailing tab, rendered one column wide (tabstop 1).
/// So a description starts after two tabs and a detail after one tab plus its
/// leading space, putting both at the same column.
fn row(e: &Entry, alias_width: usize, name_width: usize) -> String {
    let name_color = match e.kind {
        "tsm" => TSM_COLOR,
        "key" => KEY_COLOR,
        _ => TMUX_COLOR,
    };
    let detail = if e.description.is_empty() && !e.detail.is_empty() {
        format!("{DESC_COLOR} {}{RESET}", e.detail)
    } else {
        String::new()
    };
    let desc = if e.description.is_empty() {
        String::new()
    } else {
        format!("{DESC_COLOR}{}{RESET}", e.description)
    };
    format!(
        "{kind}\t{id}\t{ac}{alias:<aw$}{r} {nc}{name:<nw$}{r}\t{detail}\t{desc}",
        kind = e.kind,
        id = e.id,
        ac = ALIAS_COLOR,
        alias = e.alias,
        aw = alias_width,
        nc = name_color,
        name = e.name,
        nw = name_width,
        r = RESET,
    )
}

/// Collect every tsm subcommand via clap reflection (no hardcoded list).
fn tsm_entries() -> Vec<Entry> {
    Cli::command()
        .get_subcommands()
        .map(|sub| {
            let alias = sub.get_all_aliases().next().unwrap_or("").to_string();
            let description = sub
                .get_about()
                .map(|about| about.to_string())
                .unwrap_or_default();
            Entry {
                kind: "tsm",
                id: sub.get_name().to_string(),
                name: sub.get_name().to_string(),
                alias,
                description,
                detail: String::new(),
            }
        })
        .collect()
}

/// Collect every tmux command from `tmux list-commands`, pairing each with its
/// curated description. Aliases come from tmux (accurate for the installed
/// version); descriptions come from the embedded docs.
fn tmux_entries() -> Vec<Entry> {
    let docs = Docs::load();

    let output = match Command::new("tmux").arg("list-commands").output() {
        Ok(output) if output.status.success() => output,
        // tmux missing or failed — degrade to tsm-only rather than error out.
        _ => return Vec::new(),
    };

    String::from_utf8_lossy(&output.stdout)
        .lines()
        .filter_map(|line| {
            let (name, alias, _syntax) = parse_list_commands_line(line)?;
            let description = docs
                .tmux
                .get(&name)
                .and_then(|d| d.description.clone())
                .unwrap_or_default();
            Some(Entry {
                kind: "tmux",
                id: name.clone(),
                name,
                alias,
                description,
                detail: String::new(),
            })
        })
        .collect()
}

/// The prefix key plus one entry per key binding the user's config added or
/// changed in the `prefix` and `root` tables.
///
/// Returns `(None, [])` when tmux is missing or no server is running — the
/// prefix query doubles as that check, and gates `list-keys`, which would
/// otherwise start a server. If either binding query fails there are no
/// entries: listing every built-in binding instead would bury the user's own.
fn key_entries(tmux: &dyn Tmux) -> (Option<String>, Vec<Entry>) {
    let Some(prefix) = tmux.prefix_key().ok().flatten() else {
        return (None, Vec::new());
    };

    let entries = match (tmux.list_key_bindings(), tmux.default_key_bindings()) {
        (Ok(current), Ok(defaults)) => custom_bindings(current, &defaults)
            .into_iter()
            .map(|b| key_entry(&prefix, b))
            .collect(),
        _ => Vec::new(),
    };

    (Some(prefix), entries)
}

/// Keep the bindings that differ from tmux's defaults: keys tmux doesn't bind
/// at all, and default keys remapped to a different command.
fn custom_bindings(current: Vec<KeyBinding>, defaults: &[KeyBinding]) -> Vec<KeyBinding> {
    let defaults: HashSet<(&str, &str, &str)> = defaults
        .iter()
        .map(|b| (b.table.as_str(), b.key.as_str(), b.command.as_str()))
        .collect();

    current
        .into_iter()
        .filter(|b| !defaults.contains(&(b.table.as_str(), b.key.as_str(), b.command.as_str())))
        .collect()
}

fn key_entry(prefix: &str, binding: KeyBinding) -> Entry {
    let name = binding
        .command
        .split_whitespace()
        .next()
        .unwrap_or_default()
        .to_string();
    Entry {
        kind: "key",
        id: format!("{} {}", binding.table, binding.key),
        name,
        alias: key_display(prefix, &binding),
        description: binding.note,
        detail: binding.command,
    }
}

/// The keys to press: `C-a r` for a prefix binding, just `M-Left` for root.
fn key_display(prefix: &str, binding: &KeyBinding) -> String {
    if binding.table == "prefix" {
        format!("{prefix} {}", binding.key)
    } else {
        binding.key.clone()
    }
}

/// Parse a `tmux list-commands` line: `name (alias) [usage ...]`.
/// Returns `(name, alias, usage)`; alias is empty when absent.
fn parse_list_commands_line(line: &str) -> Option<(String, String, String)> {
    let line = line.trim();
    if line.is_empty() {
        return None;
    }

    let (name, rest) = match line.split_once(char::is_whitespace) {
        Some((name, rest)) => (name, rest.trim_start()),
        None => (line, ""), // command with no args/alias (e.g. `kill-server`)
    };

    let (alias, usage) = match rest.strip_prefix('(') {
        Some(after) => match after.split_once(')') {
            Some((alias, usage)) => (alias.to_string(), usage.trim_start()),
            None => (String::new(), rest),
        },
        None => (String::new(), rest),
    };

    Some((name.to_string(), alias, usage.to_string()))
}

/// Render a single command's tldr-style doc (title, description, examples,
/// usage) to stdout. Used both by the fzf preview and the final selection.
fn render_doc(tmux: &dyn Tmux, source: &str, name: &str) {
    match source {
        "tsm" => render_tsm_doc(name),
        "key" => render_key_doc(tmux, name),
        _ => render_tmux_doc(name),
    }
}

fn render_tsm_doc(name: &str) {
    let cmd = Cli::command();
    let sub = cmd
        .get_subcommands()
        .find(|s| s.get_name() == name || s.get_all_aliases().any(|a| a == name));

    let alias = sub
        .and_then(|s| s.get_all_aliases().next())
        .unwrap_or_default();
    let description = sub
        .and_then(|s| s.get_about())
        .map(|a| a.to_string())
        .unwrap_or_default();

    let docs = Docs::load();
    let examples = docs
        .tsm
        .get(name)
        .map(|d| d.examples.clone())
        .unwrap_or_default();

    print_doc(name, alias, &description, &examples);

    // Usage footer: the command's own `--help`, via the current binary so it
    // works off `PATH` and shows the correct `Usage: tsm <name>` prefix.
    let exe = env::current_exe().unwrap_or_else(|_| PathBuf::from("tsm"));
    if let Ok(output) = Command::new(exe).args([name, "--help"]).output()
        && output.status.success()
    {
        // Trim the leading `about` line (already shown above) so the footer
        // starts at `Usage:`, matching the tmux footer.
        let help = String::from_utf8_lossy(&output.stdout);
        let footer = match help.find("Usage:") {
            Some(idx) => &help[idx..],
            None => &help,
        };
        print_usage(footer);
    }
}

fn render_tmux_doc(name: &str) {
    let docs = Docs::load();
    let doc = docs.tmux.get(name);
    let description = doc.and_then(|d| d.description.clone()).unwrap_or_default();
    let examples = doc.map(|d| d.examples.clone()).unwrap_or_default();

    // Alias + argument syntax straight from tmux (accurate for this version).
    let (alias, syntax) = match Command::new("tmux").args(["list-commands", name]).output() {
        Ok(output) if output.status.success() => {
            let line = String::from_utf8_lossy(&output.stdout).trim().to_string();
            match parse_list_commands_line(&line) {
                Some((_, alias, syntax)) => (alias, syntax),
                None => (String::new(), String::new()),
            }
        }
        _ => (String::new(), String::new()),
    };

    print_doc(name, &alias, &description, &examples);

    if !syntax.is_empty() {
        print_usage(&format!("Usage: tmux {} {}\n", name, syntax));
    }
}

/// Render a key binding: the keys to press, its table, note, and command.
/// `id` is the `"<table> <key>"` pair from [`key_entry`].
fn render_key_doc(tmux: &dyn Tmux, id: &str) {
    let Some((table, key)) = id.split_once(' ') else {
        return;
    };
    // Gate on the prefix query so `list-keys` never starts a server.
    let Some(prefix) = tmux.prefix_key().ok().flatten() else {
        return;
    };
    let Some(binding) = tmux.list_key_bindings().ok().and_then(|bindings| {
        bindings
            .into_iter()
            .find(|b| b.table == table && b.key == key)
    }) else {
        return;
    };

    let examples = [Example {
        info: "Command bound to this key".to_string(),
        cmd: binding.command.clone(),
    }];
    print_doc(
        &key_display(&prefix, &binding),
        &binding.table,
        &binding.note,
        &examples,
    );
}

/// Print the shared tldr layout: title, description, and examples.
fn print_doc(name: &str, alias: &str, description: &str, examples: &[Example]) {
    if alias.is_empty() {
        println!("{TITLE_COLOR}{name}{RESET}");
    } else {
        println!("{TITLE_COLOR}{name}{RESET} {DESC_COLOR}({alias}){RESET}");
    }

    if !description.is_empty() {
        println!("{DESC_COLOR}{description}{RESET}");
    }

    if !examples.is_empty() {
        println!();
        for ex in examples {
            println!("{COMMENT_COLOR}# {}{RESET}", ex.info);
            println!("  {CMD_COLOR}{}{RESET}", ex.cmd);
            println!();
        }
    }
}

/// Print a dim usage/flags footer.
fn print_usage(text: &str) {
    println!("{BOLD}─────{RESET}");
    print!("{DESC_COLOR}{}{RESET}", text.trim_end());
    println!();
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::MockTmux;

    fn binding(table: &str, key: &str, note: &str, command: &str) -> KeyBinding {
        KeyBinding {
            table: table.into(),
            key: key.into(),
            note: note.into(),
            command: command.into(),
        }
    }

    #[test]
    fn custom_bindings_drops_untouched_defaults() {
        let defaults = vec![
            binding("prefix", "c", "Create a new window", "new-window"),
            binding("prefix", "%", "", "split-window -h"),
        ];
        let current = vec![
            binding("prefix", "c", "Create a new window", "new-window"),
            binding(
                "prefix",
                "%",
                "",
                "split-window -h -c \"#{pane_current_path}\"",
            ),
            binding("prefix", "r", "Reload config", "source-file ~/.tmux.conf"),
            binding("root", "M-Left", "", "select-pane -L"),
        ];

        let keys: Vec<String> = custom_bindings(current, &defaults)
            .into_iter()
            .map(|b| format!("{} {}", b.table, b.key))
            .collect();
        // `%` is a default key remapped to a new command, so it counts.
        assert_eq!(keys, vec!["prefix %", "prefix r", "root M-Left"]);
    }

    #[test]
    fn custom_bindings_matches_on_table_too() {
        // Same key and command as a prefix default, but bound in root.
        let defaults = vec![binding("prefix", "x", "", "kill-pane")];
        let current = vec![binding("root", "x", "", "kill-pane")];
        assert_eq!(custom_bindings(current, &defaults).len(), 1);
    }

    #[test]
    fn key_display_prepends_the_prefix_only_for_prefix_bindings() {
        assert_eq!(
            key_display("C-a", &binding("prefix", "r", "", "x")),
            "C-a r"
        );
        assert_eq!(
            key_display("C-a", &binding("root", "M-Left", "", "x")),
            "M-Left"
        );
    }

    #[test]
    fn key_entry_describes_with_the_note_and_keeps_the_command_as_detail() {
        let noted = key_entry(
            "C-a",
            binding("prefix", "r", "Reload", "source-file ~/.tmux.conf"),
        );
        assert_eq!(noted.kind, "key");
        assert_eq!(noted.id, "prefix r");
        assert_eq!(noted.alias, "C-a r");
        assert_eq!(noted.name, "source-file");
        assert_eq!(noted.description, "Reload");
        assert_eq!(noted.detail, "source-file ~/.tmux.conf");

        let bare = key_entry("C-a", binding("root", "M-Left", "", "select-pane -L"));
        assert_eq!(bare.id, "root M-Left");
        assert_eq!(bare.description, "");
        assert_eq!(bare.detail, "select-pane -L");
    }

    fn strip_ansi(s: &str) -> String {
        let mut out = String::new();
        let mut chars = s.chars();
        while let Some(c) = chars.next() {
            if c == '\x1b' {
                chars.by_ref().find(|&c| c == 'm');
            } else {
                out.push(c);
            }
        }
        out
    }

    #[test]
    fn row_puts_the_description_in_the_searched_field_only() {
        let noted = key_entry("C-a", binding("prefix", "r", "Reload", "source-file x"));
        let fields: Vec<String> = row(&noted, 5, 11).split('\t').map(strip_ansi).collect();
        assert_eq!(fields[0], "key");
        assert_eq!(fields[1], "prefix r");
        assert_eq!(fields[3], "", "a noted binding shows no raw command");
        assert_eq!(fields[4], "Reload");

        let bare = key_entry("C-a", binding("prefix", "%", "", "split-window -h"));
        let fields: Vec<String> = row(&bare, 5, 11).split('\t').map(strip_ansi).collect();
        assert_eq!(fields[3], " split-window -h");
        assert_eq!(
            fields[4], "",
            "the raw command must stay out of the searched field"
        );
    }

    #[test]
    fn row_aligns_description_and_detail_at_a_one_column_tabstop() {
        // With tabstop 1 every tab renders as a single column, so replacing
        // tabs with one space reproduces what fzf shows for fields 3..5.
        let shown = |e: &Entry| {
            let r = strip_ansi(&row(e, 5, 12));
            r.splitn(3, '\t').nth(2).unwrap().replace('\t', " ")
        };
        let noted = shown(&key_entry("C-a", binding("prefix", "r", "Reload", "x")));
        let bare = shown(&key_entry(
            "C-a",
            binding("prefix", "%", "", "split-window"),
        ));
        // `rfind`: the command name column also reads `split-window`.
        assert_eq!(noted.find("Reload"), bare.rfind("split-window"));
    }

    #[test]
    fn key_entries_lists_only_custom_bindings_with_the_prefix() {
        let mut tmux = MockTmux::default();
        tmux.prefix = Some("C-a".into());
        tmux.key_bindings = vec![
            binding("prefix", "c", "", "new-window"),
            binding("prefix", "r", "", "source-file ~/.tmux.conf"),
        ];
        tmux.default_key_bindings = vec![binding("prefix", "c", "", "new-window")];

        let (prefix, entries) = key_entries(&tmux);
        assert_eq!(prefix.as_deref(), Some("C-a"));
        let ids: Vec<&str> = entries.iter().map(|e| e.id.as_str()).collect();
        assert_eq!(ids, vec!["prefix r"]);
    }

    #[test]
    fn key_entries_skips_binding_queries_when_no_server_is_running() {
        let mut tmux = MockTmux::default();
        tmux.prefix = None;

        let (prefix, entries) = key_entries(&tmux);
        assert!(prefix.is_none());
        assert!(entries.is_empty());
        // `list-keys` would start a server, so it must not be reached.
        assert!(!tmux.called("list_key_bindings"));
        assert!(!tmux.called("default_key_bindings"));
    }
}
