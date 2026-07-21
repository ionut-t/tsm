//! Shared helpers for unit tests.
//!
//! The whole crate compiles into a single test binary, so a single process-wide
//! lock is enough to serialize every test that touches process environment
//! variables — without it, parallel tests reading `HOME`/`XDG_*`/`TSM_*` would
//! race against each other's mutations.

use std::sync::{Mutex, OnceLock};

fn env_lock() -> &'static Mutex<()> {
    static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
    LOCK.get_or_init(|| Mutex::new(()))
}

/// Run `f` with the given environment variables set (`Some`) or removed
/// (`None`), restoring the previous state afterwards — even if `f` panics.
///
/// Serialized across the entire test binary so env-dependent tests never race.
pub fn with_env<T>(vars: &[(&str, Option<&str>)], f: impl FnOnce() -> T) -> T {
    // Recover from a poisoned lock: a panicking test still restores its own env
    // via the guard below, so the lock protects nothing that stays corrupted.
    let _guard = env_lock().lock().unwrap_or_else(|e| e.into_inner());

    let saved: Vec<(String, Option<String>)> = vars
        .iter()
        .map(|(key, _)| ((*key).to_string(), std::env::var(key).ok()))
        .collect();

    apply_env(vars.iter().map(|(k, v)| (*k, *v)));

    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(f));

    apply_env(saved.iter().map(|(k, v)| (k.as_str(), v.as_deref())));

    match result {
        Ok(value) => value,
        Err(payload) => std::panic::resume_unwind(payload),
    }
}

fn apply_env<'a>(vars: impl Iterator<Item = (&'a str, Option<&'a str>)>) {
    for (key, value) in vars {
        // SAFETY: all env mutation in the test binary flows through `with_env`,
        // which holds `env_lock` for the duration, so no other thread reads or
        // writes the environment concurrently.
        unsafe {
            match value {
                Some(val) => std::env::set_var(key, val),
                None => std::env::remove_var(key),
            }
        }
    }
}

use std::cell::{Cell, RefCell};
use std::collections::{HashMap, VecDeque};

use crate::error::{Result, TsmError};
use crate::fzf::{Picker, PickerOptions};
use crate::history::HistoryStore;
use crate::tmux::{Tmux, Window};
use crate::zoxide::DirectorySource;

/// In-memory [`HistoryStore`] double.
///
/// Timestamps come from a monotonic counter rather than the wall clock, so each
/// `record` is strictly newer than the last — making history-ordering tests
/// deterministic and instant, with no filesystem and no `sleep()`.
pub struct InMemoryHistory {
    entries: HashMap<String, u128>,
    next: u128,
}

impl InMemoryHistory {
    pub fn new() -> Self {
        Self {
            entries: HashMap::new(),
            next: 1,
        }
    }

    /// Pre-populate with `(session, window_index, timestamp)` entries. The
    /// internal clock starts just above the largest seed, so later `record`s
    /// always outrank the seeds.
    pub fn seeded(entries: &[(&str, u32, u128)]) -> Self {
        let mut map = HashMap::new();
        let mut max = 0;
        for (session, index, ts) in entries {
            map.insert(format!("{session}:{index}"), *ts);
            max = max.max(*ts);
        }
        Self {
            entries: map,
            next: max + 1,
        }
    }
}

impl HistoryStore for InMemoryHistory {
    fn last_access(&self, session: &str, window_index: u32) -> Option<u128> {
        self.entries
            .get(&format!("{session}:{window_index}"))
            .copied()
    }

    fn record(&mut self, session: &str, window_index: u32) -> Result<()> {
        let ts = self.next;
        self.next += 1;
        self.entries.insert(format!("{session}:{window_index}"), ts);
        Ok(())
    }
}

/// In-memory [`DirectorySource`] double returning a fixed directory list (or a
/// failure), so `new`'s directory-selection flow can be tested without zoxide.
pub struct MockDirectories {
    dirs: Vec<String>,
    fail: bool,
}

impl MockDirectories {
    /// A source that returns the given directories.
    pub fn with(dirs: &[&str]) -> Self {
        Self {
            dirs: dirs.iter().map(|d| d.to_string()).collect(),
            fail: false,
        }
    }

    /// A source that reports zoxide is unavailable.
    pub fn failing() -> Self {
        Self {
            dirs: Vec::new(),
            fail: true,
        }
    }
}

impl DirectorySource for MockDirectories {
    fn query_directories(&self) -> Result<Vec<String>> {
        if self.fail {
            Err(TsmError::ZoxideQueryFailed)
        } else {
            Ok(self.dirs.clone())
        }
    }
}

/// In-memory [`Picker`] double that returns scripted selections instead of
/// spawning `fzf`.
///
/// Each `pick` pops the next queued response (`Some(selection)` for a choice,
/// `None` for a cancellation) and records the items it was shown, so tests can
/// assert both what the picker presented and how the command reacted.
pub struct MockPicker {
    responses: RefCell<VecDeque<Option<String>>>,
    shown: RefCell<Vec<Vec<String>>>,
}

impl MockPicker {
    /// A picker that returns the given responses in order; once exhausted,
    /// further calls behave as cancellations.
    pub fn scripted(responses: Vec<Option<String>>) -> Self {
        Self {
            responses: RefCell::new(responses.into()),
            shown: RefCell::new(Vec::new()),
        }
    }

    /// A picker whose first selection is `selection`.
    pub fn returning(selection: &str) -> Self {
        Self::scripted(vec![Some(selection.to_string())])
    }

    /// A picker that always cancels (user pressed Esc).
    pub fn cancelling() -> Self {
        Self::scripted(vec![])
    }

    /// The item lists shown to the picker, in call order.
    pub fn shown(&self) -> Vec<Vec<String>> {
        self.shown.borrow().clone()
    }
}

impl Picker for MockPicker {
    fn pick(&self, _options: &PickerOptions, items: &[String]) -> Result<Option<String>> {
        self.shown.borrow_mut().push(items.to_vec());
        Ok(self.responses.borrow_mut().pop_front().flatten())
    }
}

/// In-memory [`Tmux`] double for exercising command `.run()` methods without a
/// real tmux server.
///
/// Query methods return whatever the test configured on the public fields;
/// every method also appends a human-readable record to `calls`, so tests can
/// assert on the exact tmux operations a command performed and in what order.
pub struct MockTmux {
    pub inside_tmux: bool,
    pub sessions: Vec<String>,
    /// When set, `list_sessions` fails (models tmux missing / spawn failure),
    /// so tests can assert a command surfaces the error instead of swallowing it.
    pub fail_list_sessions: bool,
    pub windows: Vec<Window>,
    pub current_session: String,
    /// Returned by `get_current_window` / `get_current_window_index`.
    pub current_window: (String, u32),
    /// Returned by `get_pane_id`.
    pub pane_id: String,
    /// Returned by `list_panes`.
    pub panes: Vec<String>,
    /// Window index reported by `new_window`.
    pub new_window_index: usize,
    calls: RefCell<Vec<String>>,
    /// Monotonic counter so each split returns a distinct pane id (`%p1`, …),
    /// letting layout tests verify which pane subsequent operations target.
    next_split: Cell<u32>,
}

impl Default for MockTmux {
    fn default() -> Self {
        Self {
            inside_tmux: true,
            sessions: Vec::new(),
            fail_list_sessions: false,
            windows: Vec::new(),
            current_session: String::new(),
            current_window: (String::new(), 0),
            pane_id: "%0".to_string(),
            panes: vec!["%0".to_string()],
            new_window_index: 1,
            calls: RefCell::new(Vec::new()),
            next_split: Cell::new(1),
        }
    }
}

impl MockTmux {
    fn log(&self, entry: impl Into<String>) {
        self.calls.borrow_mut().push(entry.into());
    }

    /// Allocate the next distinct pane id returned by a split.
    fn next_pane(&self) -> String {
        let n = self.next_split.get();
        self.next_split.set(n + 1);
        format!("%p{n}")
    }

    /// The ordered list of operations performed so far.
    pub fn calls(&self) -> Vec<String> {
        self.calls.borrow().clone()
    }

    /// Whether any recorded call starts with `prefix`.
    pub fn called(&self, prefix: &str) -> bool {
        self.calls.borrow().iter().any(|c| c.starts_with(prefix))
    }
}

impl Tmux for MockTmux {
    fn is_inside_tmux(&self) -> bool {
        self.inside_tmux
    }

    fn new_window(
        &self,
        session: &str,
        name: Option<&str>,
        _path: Option<&std::path::Path>,
        _env: &HashMap<String, String>,
    ) -> Result<usize> {
        self.log(format!("new_window({session},{})", name.unwrap_or("-")));
        Ok(self.new_window_index)
    }

    fn rename_window(&self, session: &str, new_name: &str) -> Result<()> {
        self.log(format!("rename_window({session},{new_name})"));
        Ok(())
    }

    fn split_horizontal(
        &self,
        target_pane: &str,
        _path: Option<&std::path::Path>,
        _percentage: Option<u32>,
        _env: &HashMap<String, String>,
    ) -> Result<String> {
        let id = self.next_pane();
        self.log(format!("split_horizontal({target_pane}->{id})"));
        Ok(id)
    }

    fn split_vertical(
        &self,
        target_pane: &str,
        _path: Option<&std::path::Path>,
        _percentage: Option<u32>,
        _env: &HashMap<String, String>,
    ) -> Result<String> {
        let id = self.next_pane();
        self.log(format!("split_vertical({target_pane}->{id})"));
        Ok(id)
    }

    fn resize_pane_height(&self, pane_id: &str, percentage: u32) -> Result<()> {
        self.log(format!("resize_pane_height({pane_id},{percentage})"));
        Ok(())
    }

    fn resize_pane_width(&self, pane_id: &str, percentage: u32) -> Result<()> {
        self.log(format!("resize_pane_width({pane_id},{percentage})"));
        Ok(())
    }

    fn send_keys(&self, pane_id: &str, command: &str) -> Result<()> {
        self.log(format!("send_keys({pane_id},{command})"));
        Ok(())
    }

    fn select_pane(&self, pane_id: &str) -> Result<()> {
        self.log(format!("select_pane({pane_id})"));
        Ok(())
    }

    fn list_panes(&self, _session: &str, _window_index: usize) -> Result<Vec<String>> {
        Ok(self.panes.clone())
    }

    fn get_current_window_index(&self, _session: &str) -> Result<usize> {
        Ok(self.current_window.1 as usize)
    }

    fn current_session(&self) -> Result<String> {
        Ok(self.current_session.clone())
    }

    fn list_sessions(&self) -> Result<Vec<String>> {
        if self.fail_list_sessions {
            return Err(TsmError::TmuxCommand("tmux not found".to_string()));
        }
        Ok(self.sessions.clone())
    }

    fn list_windows(&self) -> Result<Vec<Window>> {
        Ok(self.windows.clone())
    }

    fn create_session_detached(
        &self,
        name: &str,
        _path: &std::path::Path,
        _env: &HashMap<String, String>,
    ) -> Result<()> {
        self.log(format!("create_session_detached({name})"));
        Ok(())
    }

    fn respawn_pane(
        &self,
        pane_id: &str,
        _path: &std::path::Path,
        _env: &HashMap<String, String>,
    ) -> Result<()> {
        self.log(format!("respawn_pane({pane_id})"));
        Ok(())
    }

    fn new_session(&self, name: String, path: String) -> Result<()> {
        self.log(format!("new_session({name},{path})"));
        Ok(())
    }

    fn select_window(&self, session: &str, window_index: usize) -> Result<()> {
        self.log(format!("select_window({session},{window_index})"));
        Ok(())
    }

    fn kill_session(&self, session: &str) -> Result<()> {
        self.log(format!("kill_session({session})"));
        Ok(())
    }

    fn kill_all_sessions(&self) -> Result<()> {
        self.log("kill_all_sessions()");
        Ok(())
    }

    fn rename_session(&self, current_name: Option<&str>, new_name: &str) -> Result<()> {
        self.log(format!(
            "rename_session({},{new_name})",
            current_name.unwrap_or("-")
        ));
        Ok(())
    }

    fn attach_session(&self, session: &str) -> Result<()> {
        self.log(format!("attach_session({session})"));
        Ok(())
    }

    fn switch_session(&self, name: &str) -> Result<()> {
        self.log(format!("switch_session({name})"));
        Ok(())
    }

    fn switch_to_window(&self, session: &str, window_index: u32) -> Result<()> {
        self.log(format!("switch_to_window({session},{window_index})"));
        Ok(())
    }

    fn attach_to_window(&self, session: &str, window_index: u32) -> Result<()> {
        self.log(format!("attach_to_window({session},{window_index})"));
        Ok(())
    }

    fn get_current_window(&self) -> Result<(String, u32)> {
        Ok(self.current_window.clone())
    }

    fn move_window(&self, from_session: &str, from_index: u32, to_session: &str) -> Result<()> {
        self.log(format!(
            "move_window({from_session}:{from_index}->{to_session})"
        ));
        Ok(())
    }

    fn get_pane_id(&self, _session: &str, _window_index: u32) -> Result<String> {
        Ok(self.pane_id.clone())
    }

    fn find_window_by_pane_id(&self, pane_id: &str) -> Result<(String, u32)> {
        self.windows
            .iter()
            .find(|w| w.pane_id == pane_id)
            .map(|w| (w.session_name.clone(), w.index))
            .ok_or_else(|| TsmError::TmuxCommand("pane id not found".to_string()))
    }

    fn swap_windows(&self, source_index: u32, target_index: u32) -> Result<()> {
        self.log(format!("swap_windows({source_index},{target_index})"));
        Ok(())
    }

    fn is_last_window_in_session(&self, session: &str) -> Result<bool> {
        let count = self
            .windows
            .iter()
            .filter(|w| w.session_name == session)
            .count();
        Ok(count == 1)
    }

    fn display_message(&self, message: &str) -> Result<()> {
        self.log(format!("display_message({message})"));
        Ok(())
    }
}
