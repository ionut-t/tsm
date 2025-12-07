use std::io;
use thiserror::Error;

#[derive(Error, Debug)]
pub enum TsmError {
    #[error("not inside a tmux session")]
    NotInTmux,

    #[error("failed to execute tmux command: {0}")]
    TmuxCommand(String),

    #[error("IO error: {0}")]
    Io(#[from] io::Error),
}

pub type Result<T> = std::result::Result<T, TsmError>;
