mod cli;
mod error;
mod fzf;
mod history;
mod paths;
mod tmux;
mod zoxide;

use clap::Parser;
use cli::Cli;
use tmux::TmuxClient;

fn main() -> error::Result<()> {
    let cli = Cli::parse();
    let client = TmuxClient::new();

    if let Err(e) = cli.run(client) {
        let error_client = TmuxClient::new();
        let _ = error_client.display_message(&format!("Error: {}", e));

        return Err(e);
    }

    Ok(())
}
