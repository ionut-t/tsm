use clap::{Parser, Subcommand, command};

use crate::{
    cli::{
        kill::KillCommand, last_session::LastSessionCommand, last_window::LastWindowCommand,
        move_window::MoveWindowCommand, new::NewCommand, record::RecordCommand,
        rename::RenameCommand, swap::SwapWindowCommand, switch::SwitchCommand,
        switch_windows::SwitchWindowCommand,
    },
    error::Result,
    tmux::TmuxClient,
};

/// A CLI for managing tmux sessions and windows
#[derive(Parser)]
#[command(name = "tsm")]
#[command(about = "A CLI for managing tmux sessions", long_about = None)]
#[command(version)]
#[command(subcommand_required(true))]
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
    #[command(alias = "record")]
    Record(RecordCommand),

    /// Move window to another session
    #[command(alias = "mv")]
    MoveWindow(MoveWindowCommand),

    /// Swap two windows in the same session
    SwapWindow(SwapWindowCommand),
}

impl Cli {
    pub fn run(&self, client: TmuxClient) -> Result<()> {
        match &self.command {
            Commands::New(cmd) => cmd.run(&client),
            Commands::Kill(cmd) => cmd.run(&client),
            Commands::Rename(cmd) => cmd.run(&client),
            Commands::Switch(cmd) => cmd.run(&client),
            Commands::SwitchWindow(cmd) => cmd.run(&client),
            Commands::LastSession(cmd) => cmd.run(&client),
            Commands::LastWindow(cmd) => cmd.run(&client),
            Commands::Record(cmd) => cmd.run(&client),
            Commands::MoveWindow(cmd) => cmd.run(&client),
            Commands::SwapWindow(cmd) => cmd.run(&client),
        }
    }
}
