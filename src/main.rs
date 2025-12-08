mod cli;
mod commands;
mod error;
mod fzf;
mod history;
mod tmux;
mod zoxide;

use clap::Parser;
use cli::Cli;
use tmux::TmuxClient;

fn main() {
    let cli = Cli::parse();
    let client = TmuxClient::new();

    if let Some(command) = cli.command {
        if let Err(e) = handle_command(command, &client) {
            eprintln!("Error: {}", e);
            std::process::exit(1);
        }
    } else {
        eprintln!("No command provided. Use --help for more information");
    }
}

fn handle_command(command: cli::Commands, client: &TmuxClient) -> error::Result<()> {
    use cli::Commands;

    match command {
        Commands::New {
            name,
            path,
            preview,
        } => commands::new::handle(client, name, path, preview),
        Commands::Kill { session } => commands::kill::handle(client, session),
        Commands::Rename {
            current_name,
            new_name,
        } => commands::rename::handle(client, current_name, new_name),
        Commands::Switch { name } => commands::switch::handle(client, name),
        Commands::SwitchWindow => commands::switch_windows::handle(client),
    }
}
