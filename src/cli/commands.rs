use clap::{Parser, Subcommand, command};

/// A CLI for managing tmux sessions and windows
#[derive(Parser)]
#[command(name = "tsm")]
#[command(about = "A CLI for managing tmux sessions", long_about = None)]
#[command(version)]
#[command(subcommand_required(true))]
pub struct Cli {
    /// The command to run
    #[clap(subcommand)]
    pub command: Option<Commands>,

    /// The name of the task
    #[clap(short, long)]
    pub name: Option<String>,

    /// The description of the task
    #[clap(short, long)]
    pub description: Option<String>,
}

/// Available commands for the CLI
#[derive(Subcommand, Debug)]
pub enum Commands {
    /// Create a new tmux session
    #[command(alias = "n")]
    New {
        /// Session name (auto-generated if not provided)
        #[clap(short, long)]
        name: Option<String>,

        /// Directory path (skips zoxide if provided)
        #[clap(short, long)]
        path: Option<String>,

        /// Show directory preview in fzf
        #[clap(short = 'v', long, default_value_t = false)]
        preview: bool,
    },

    /// Kill session
    #[command(alias = "k")]
    Kill {
        /// Session name
        #[clap(short, long)]
        session: Option<String>,
    },

    /// Rename session
    #[command(alias = "r")]
    Rename {
        /// Current name - defaults to the active session if not provided
        #[clap(short = 'c', long)]
        current_name: Option<String>,
        /// New name
        #[clap(short = 'n', long)]
        new_name: String,
    },

    /// Switch to session
    #[command(alias = "s")]
    Switch {
        /// Name of the session to switch to
        #[clap(short, long)]
        name: Option<String>,
    },

    /// Switch to a window
    #[command(alias = "sw")]
    SwitchWindow,
}
