mod cli;
mod error;
mod tmux;

use clap::Parser;
use cli::Cli;
use tmux::{Client, TmuxClient};

fn main() {
    let cli = Cli::parse();

    match cli.command {
        Some(command) => {
            let client = TmuxClient;

            match command {
                cli::Commands::List => {
                    let sessions = client.list_sessions();
                    for session in sessions {
                        println!("{}", session);
                    }
                }

                cli::Commands::New { name, path } => {
                    match client.new_session(name.as_deref(), path.as_deref()) {
                        Ok(_) => display_message(
                            &client,
                            &format!("Session '{}' created", name.as_deref().unwrap_or("default")),
                        ),
                        Err(e) => eprintln!("Error creating session: {}", e),
                    }
                }

                cli::Commands::Kill { session } => match client.kill_session(session.as_deref()) {
                    Ok(_) => display_message(
                        &client,
                        &format!(
                            "Session '{}' killed",
                            session.as_deref().unwrap_or("default")
                        ),
                    ),
                    Err(e) => eprintln!("Error killing session: {}", e),
                },

                cli::Commands::Rename {
                    current_name,
                    new_name,
                } => match client.rename_session(current_name.as_deref(), &new_name) {
                    Ok(_) => {
                        display_message(&client, &format!("Session renamed to '{}'", new_name))
                    }
                    Err(e) => eprintln!("Error renaming session: {}", e),
                },

                cli::Commands::Switch { name } => {
                    if client.is_inside_tmux() {
                        match client.switch_session(&name) {
                            Ok(_) => {
                                display_message(&client, &format!("Switched to session '{}'", name))
                            }
                            Err(e) => eprintln!("Error switching session: {}", e),
                        }
                    } else {
                        match client.attach_session(&name) {
                            Ok(_) => {}
                            Err(e) => eprintln!("Error attaching to session: {}", e),
                        }
                    }
                }
            }
        }
        None => {
            eprintln!("No command provided. Use --help for more information");
        }
    }
}

fn display_message(tmux: &TmuxClient, message: &str) {
    if tmux.is_inside_tmux() {
        tmux.display_message(message);
    } else {
        println!("{}", message);
    }
}
