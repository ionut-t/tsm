use std::io;

use clap::CommandFactory;
use clap_complete::Shell;

use super::commands::Cli;

#[derive(clap::Args, Debug)]
pub struct CompletionsCommand {
    /// The shell to generate completions for
    pub shell: Shell,
}

impl CompletionsCommand {
    pub fn run(&self) {
        let mut cmd = Cli::command();
        clap_complete::generate(self.shell, &mut cmd, "tsm", &mut io::stdout());
    }
}
