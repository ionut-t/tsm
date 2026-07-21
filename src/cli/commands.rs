use clap::{Parser, Subcommand, command};

use crate::{
    cli::{
        completions::CompletionsCommand, help::HelpCommand, kill::KillCommand,
        last_session::LastSessionCommand, last_window::LastWindowCommand,
        move_window::MoveWindowCommand, new::NewCommand, record::RecordCommand,
        rename::RenameCommand, swap::SwapWindowCommand, switch::SwitchCommand,
        switch_windows::SwitchWindowCommand, workspace::WorkspaceCommand,
    },
    error::Result,
    fzf::FzfPicker,
    tmux::TmuxClient,
    zoxide::Zoxide,
};

/// A CLI for managing tmux sessions and windows
#[derive(Parser)]
#[command(name = "tsm")]
#[command(about = "A CLI for managing tmux sessions", long_about = None)]
#[command(version)]
#[command(subcommand_required(true))]
#[command(disable_help_subcommand = true)]
pub struct Cli {
    /// The command to run
    #[clap(subcommand)]
    pub command: Commands,
}

/// Available commands for the CLI
#[derive(Subcommand, Debug)]
pub enum Commands {
    /// Create a new tmux session
    #[command(alias = "n")]
    New(NewCommand),

    /// Kill session
    #[command(alias = "k")]
    Kill(KillCommand),

    /// Rename session
    #[command(alias = "r")]
    Rename(RenameCommand),

    /// Switch to session
    #[command(alias = "s")]
    Switch(SwitchCommand),

    /// Switch to a window
    #[command(alias = "sw")]
    SwitchWindow(SwitchWindowCommand),

    /// Switch to the last active session
    #[command(alias = "ls")]
    LastSession(LastSessionCommand),

    /// Switch to the last active window
    #[command(alias = "lw")]
    LastWindow(LastWindowCommand),

    /// Record window history
    Record(RecordCommand),

    /// Move window to another session
    #[command(alias = "mv")]
    MoveWindow(MoveWindowCommand),

    /// Swap two windows in the same session
    #[command(alias = "sww")]
    SwapWindow(SwapWindowCommand),

    /// Workspace
    #[command(alias = "ws")]
    Workspace(WorkspaceCommand),

    /// Generate shell completions
    Completions(CompletionsCommand),

    /// Browse all tsm and tmux commands
    #[command(alias = "h")]
    Help(HelpCommand),
}

impl Cli {
    pub fn run(&self, client: TmuxClient) -> Result<()> {
        let picker = FzfPicker::new();
        match &self.command {
            Commands::New(cmd) => cmd.run(&client, &picker, &Zoxide::new()),
            Commands::Kill(cmd) => cmd.run(&client, &picker),
            Commands::Rename(cmd) => cmd.run(&client),
            Commands::Switch(cmd) => cmd.run(&client, &picker),
            Commands::SwitchWindow(cmd) => cmd.run(&client, &picker),
            Commands::LastSession(cmd) => cmd.run(&client),
            Commands::LastWindow(cmd) => cmd.run(&client),
            Commands::Record(cmd) => cmd.run(&client),
            Commands::MoveWindow(cmd) => cmd.run(&client, &picker),
            Commands::SwapWindow(cmd) => cmd.run(&client),
            Commands::Workspace(cmd) => cmd.run(&client, &picker),
            Commands::Completions(cmd) => {
                cmd.run();
                Ok(())
            }
            Commands::Help(cmd) => cmd.run(&picker),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::CommandFactory;

    #[test]
    fn cli_definition_is_valid() {
        // clap's own consistency check: duplicate flags, bad arg config, etc.
        Cli::command().debug_assert();
    }

    #[test]
    fn requires_a_subcommand() {
        assert!(Cli::try_parse_from(["tsm"]).is_err());
    }

    #[test]
    fn parses_new_command_with_flags() {
        let cli = Cli::try_parse_from(["tsm", "new", "--name", "dev", "--quiet"]).unwrap();
        assert!(matches!(cli.command, Commands::New(_)));
    }

    #[test]
    fn short_aliases_resolve_to_expected_subcommands() {
        assert!(matches!(
            Cli::try_parse_from(["tsm", "n"]).unwrap().command,
            Commands::New(_)
        ));
        assert!(matches!(
            Cli::try_parse_from(["tsm", "sw"]).unwrap().command,
            Commands::SwitchWindow(_)
        ));
        assert!(matches!(
            Cli::try_parse_from(["tsm", "mv"]).unwrap().command,
            Commands::MoveWindow(_)
        ));
        assert!(matches!(
            Cli::try_parse_from(["tsm", "ws"]).unwrap().command,
            Commands::Workspace(_)
        ));
    }

    #[test]
    fn swap_window_requires_target() {
        // `--target` has no default and is mandatory.
        assert!(Cli::try_parse_from(["tsm", "swap-window"]).is_err());
        assert!(Cli::try_parse_from(["tsm", "swap-window", "--target", "2"]).is_ok());
    }

    #[test]
    fn rejects_unknown_subcommand() {
        assert!(Cli::try_parse_from(["tsm", "bogus"]).is_err());
    }
}
