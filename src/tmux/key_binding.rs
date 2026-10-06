/// A single tmux key binding, as reported by `tmux list-keys`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct KeyBinding {
    /// Key table the binding lives in (`prefix` or `root`).
    pub table: String,
    /// Key name as tmux prints it (e.g. `r`, `M-Left`, `C-a`).
    pub key: String,
    /// Description set with `bind -N "..."`; empty if none.
    pub note: String,
    /// The command (or `\;`-separated command list) the key runs.
    pub command: String,
}
