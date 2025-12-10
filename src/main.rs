mod cli;
mod commands;
mod error;
mod fzf;
mod history;
mod paths;
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
            prompt,
        } => commands::new::handle(client, name, path, preview, prompt),
        Commands::Kill {
            session,
            prompt,
            all,
        } => commands::kill::handle(client, session, prompt, all),
        Commands::Rename {
            current_name,
            new_name,
        } => commands::rename::handle(client, current_name, new_name),
        Commands::Switch { name, prompt } => commands::switch::handle(client, name, prompt),
        Commands::SwitchWindow { prompt, preview } => {
            commands::switch_windows::handle(client, prompt, preview)
        }
        Commands::LastSession => commands::last_session::handle(client),
        Commands::LastWindow => commands::last_window::handle(client),
    }
}
