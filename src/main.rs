mod cli;
mod error;
mod fzf;
mod history;
mod tmux;
mod workspace;
mod zoxide;

use clap::Parser;
use cli::Cli;
use tmux::TmuxClient;

fn main() -> error::Result<()> {
    let cli = Cli::parse();
    let client = TmuxClient::new();

    if let Err(e) = cli.run(client) {
        return TmuxClient::new().display_message(&format!("Error: {}", e));
    }

    Ok(())
}
