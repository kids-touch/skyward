//! Data rescue: finding files, copying them out and reporting WeChat recovery.

use super::*;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RescueState {
    pub kinds: [bool; 4],
    pub running: bool,
    pub exporting: bool,
    pub stopping: bool,
    pub seen: u64,
    pub found: u64,
    pub bytes: u64,
    pub at: String,
    pub saved: Option<String>,
    pub outcome: Option<String>,
}

impl Default for RescueState {
    fn default() -> Self {
        Self { kinds: [true, true, true, true], running: false, exporting: false, stopping: false,
            seen: 0, found: 0, bytes: 0, at: String::new(), saved: None, outcome: None }
    }
}

impl App {
    pub fn rescue_started(&mut self, exporting: bool) {
        let r = &mut self.state.rescue;
        r.running = true;
        r.exporting = exporting;
        r.stopping = false;
        r.seen = 0;
        r.bytes = 0;
        r.at.clear();
        r.outcome = None;
        if !exporting { r.found = 0; r.saved = None; }
        self.refresh();
    }

    pub fn rescue_progress(&mut self, seen: u64, found: u64, bytes: u64, at: String) {
        let r = &mut self.state.rescue;
        r.seen = seen; r.found = found; r.bytes = bytes; r.at = at;
        self.refresh();
    }

    pub fn rescue_found(&mut self, found: u64) {
        self.state.rescue.found = found;
        self.refresh();
    }

    pub fn rescue_saved(&mut self, saved: String) {
        self.state.rescue.saved = Some(saved);
        self.refresh();
    }

    pub fn rescue_ended(&mut self, outcome: String, now: u64) {
        let r = &mut self.state.rescue;
        r.running = false; r.exporting = false; r.stopping = false;
        r.outcome = Some(outcome);
        self.wake(now);
        self.refresh();
    }
}
