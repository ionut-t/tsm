mod cli;
mod error;
mod fzf;
mod history;
mod tmux;
mod workspace;
mod zoxide;

#[cfg(test)]
mod test_support;

use std::process::ExitCode;

use clap::Parser;
use cli::Cli;
use tmux::{Tmux, TmuxClient};

fn main() -> ExitCode {
    let cli = Cli::parse();
    let client = TmuxClient::new();

    if let Err(e) = cli.run(client) {
        let message = format!("Error: {}", e);
        // display_message reaches the user inside popups and keybindings (and
        // prints to the terminal outside tmux); fall back to stderr if it fails.
        if let Err(display_err) = TmuxClient::new().display_message(&message) {
            eprintln!("{}", message);
            eprintln!("(failed to display error in tmux: {})", display_err);
        }
        return ExitCode::FAILURE;
    }

    ExitCode::SUCCESS
}
