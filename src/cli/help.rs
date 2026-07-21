use std::env;
use std::path::PathBuf;
use std::process::Command;

use clap::CommandFactory;

use crate::cli::help_docs::{Docs, Example};
use crate::cli::utils::shell_quote;
use crate::error::Result;
use crate::fzf::{Picker, PickerOptions};

use super::commands::Cli;

// ANSI styling for the picker rows (fzf is launched with `--ansi`).
const ALIAS_COLOR: &str = "\x1b[35m"; // magenta — the keybinding-analog column
const TSM_COLOR: &str = "\x1b[36m"; // cyan — tsm command names
const TMUX_COLOR: &str = "\x1b[32m"; // green — tmux command names
const DESC_COLOR: &str = "\x1b[2m"; // dim — descriptions
const RESET: &str = "\x1b[0m";

// Styling for the rendered tldr-style doc (preview pane + final output).
const BOLD: &str = "\x1b[1m";
const TITLE_COLOR: &str = "\x1b[1;36m"; // bold cyan
const COMMENT_COLOR: &str = "\x1b[2m"; // dim — example descriptions
const CMD_COLOR: &str = "\x1b[32m"; // green — example commands

/// A single command entry, before it is laid out into aligned columns.
struct Entry {
    /// `"tsm"` or `"tmux"` — the hidden key that routes help/preview.
    kind: &'static str,
    /// Canonical command name.
    name: String,
    /// Short alias (Helix-style keybinding column); empty if none.
    alias: String,
    /// One-line human description shown in the list.
    description: String,
}

/// Browse all tsm and tmux commands in a Helix-style fuzzy picker.
///
/// Lists every tsm subcommand and every tmux command with a short description,
/// and shows tldr-style usage examples in the preview. Selecting a command
/// prints its examples and usage.
#[derive(clap::Parser, Debug)]
pub struct HelpCommand {
    /// fzf prompt
    #[clap(short = 'P', long, default_value = "Command: ")]
    prompt: String,

    /// Internal: render the tldr doc for a single command (used by the preview).
    #[clap(long, hide = true)]
    render: Option<String>,

    /// Internal: source of the `--render` command (`tsm` or `tmux`).
    #[clap(long, hide = true, default_value = "tsm")]
    source: String,
}

impl HelpCommand {
    pub fn run(&self, picker: &dyn Picker) -> Result<()> {
        // Preview / doc-render mode: print one command's doc and exit.
        if let Some(name) = &self.render {
            render_doc(&self.source, name);
            return Ok(());
        }

        let mut entries = tsm_entries();
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

        // Each row is four tab-delimited fields:
        //   1: kind     — hidden; routes the preview and the final doc render.
        //   2: name     — hidden; the canonical command name.
        //   3: keys      — shown AND searched: aligned `alias · command` columns.
        //   4: desc      — shown but NOT searched (so a query matches command
        //                  names/aliases, not stray letters in descriptions).
        let items = entries
            .iter()
            .map(|e| {
                let name_color = if e.kind == "tsm" {
                    TSM_COLOR
                } else {
                    TMUX_COLOR
                };
                format!(
                    "{kind}\t{name}\t{ac}{alias:<aw$}{r} {nc}{name_disp:<nw$}{r}\t{dc}{desc}{r}",
                    kind = e.kind,
                    name = e.name,
                    ac = ALIAS_COLOR,
                    alias = e.alias,
                    aw = alias_width,
                    nc = name_color,
                    name_disp = e.name,
                    nw = name_width,
                    dc = DESC_COLOR,
                    desc = e.description,
                    r = RESET,
                )
            })
            .collect::<Vec<String>>();

        let header = format!(
            "{dc}{alias:<aw$} {name:<nw$}\t{desc}{r}",
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

        let options = PickerOptions::new()
            .with_prompt(&self.prompt)
            .with_delimiter("\t")
            // Show the keys column (3) and the description (4)...
            .with_nth("3,4")
            // ...but search only the keys column. `--nth` counts fields left
            // after `--with-nth`, so field 1 here is the alias+command column,
            // keeping matches off stray letters in descriptions.
            .with_search_nth("1")
            .no_hscroll()
            .with_preview_command(&preview_cmd)
            .with_preview_window("right:55%:border-rounded")
            .with_preview_label(" Help ")
            .with_border("rounded")
            .with_border_label(" Commands ")
            .with_header(&header);

        let selection = match picker.pick(&options, &items)? {
            Some(sel) => sel,
            None => return Ok(()), // User canceled
        };

        let mut fields = selection.split('\t');
        let kind = fields.next().unwrap_or("");
        let name = fields.next().unwrap_or("");
        if name.is_empty() {
            return Ok(());
        }

        // Print the same tldr doc the preview showed.
        render_doc(kind, name);
        Ok(())
    }
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
                name: sub.get_name().to_string(),
                alias,
                description,
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
                name,
                alias,
                description,
            })
        })
        .collect()
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
fn render_doc(source: &str, name: &str) {
    if source == "tsm" {
        render_tsm_doc(name);
    } else {
        render_tmux_doc(name);
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
