pub mod commands;
pub mod completions;
pub mod help;
pub mod help_docs;
pub mod kill;
pub mod last_session;
pub mod last_window;
pub mod move_window;
pub mod new;
pub mod record;
pub mod rename;
pub mod swap;
pub mod switch;
pub mod switch_windows;
mod utils;
pub mod workspace;

pub use commands::Cli;
