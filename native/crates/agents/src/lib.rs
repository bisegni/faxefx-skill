//! Normalized provider boundary; availability never implies permission.
use axe_domain::Context;
use serde::{Deserialize, Serialize};
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Provider {
    Codex,
    Claude,
}
#[derive(Debug, Clone, PartialEq)]
pub enum Event {
    Text {
        turn: u64,
        text: String,
    },
    Activity {
        turn: u64,
        id: String,
        summary: String,
    },
    Ready {
        skill_version: String,
    },
    Interrupted {
        turn: u64,
    },
    Failed(String),
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tool {
    InspectSession,
    InspectBlock,
    AttachedAnalysis,
    SetParameters,
    TransactionStatus,
}
/// This is the complete host-tool surface. Save/restore/raw MIDI are absent.
pub const TOOLS: &[(&str, Tool)] = &[
    ("session_inspect", Tool::InspectSession),
    ("block_inspect", Tool::InspectBlock),
    ("analysis_attached", Tool::AttachedAnalysis),
    ("parameters_set", Tool::SetParameters),
    ("transaction_status", Tool::TransactionStatus),
];
pub const SKILL: &str = include_str!("../../../resources/axefx-live/SKILL.md");
pub trait AgentAdapter: Send {
    fn provider(&self) -> Provider;
    fn available(&self) -> Result<(), String>;
    fn connect(&mut self) -> Result<(), String>;
    fn initialize_skill(&mut self, skill: &str) -> Result<Vec<Event>, String>;
    fn start(&mut self, context: &Context, resume: Option<&str>) -> Result<String, String>;
    fn submit(&mut self, turn: u64, message: &str) -> Result<(), String>;
    fn poll(&mut self) -> Vec<Event>;
    fn interrupt(&mut self) -> Result<(), String>;
    fn disconnect(&mut self);
}
#[derive(Debug, Default)]
pub struct Startup {
    context: Option<Context>,
    provider: Option<Provider>,
    skill_ready: bool,
    turn: u64,
    active: bool,
}
impl Startup {
    pub fn captured(&mut self, c: Context) {
        self.stop();
        self.context = Some(c);
        self.provider = None;
        self.skill_ready = false;
    }
    pub fn choose(&mut self, p: Provider) -> Result<(), String> {
        if self.context.is_none() {
            return Err("Read a device or explicitly select Test mock first".into());
        }
        self.stop();
        self.provider = Some(p);
        self.skill_ready = false;
        Ok(())
    }
    pub fn skill_loaded(&mut self, version: &str, isolation_verified: bool) -> Result<(), String> {
        if self.provider.is_none() || version != "1" || !isolation_verified {
            return Err("Skill/version/tool isolation is unverified".into());
        }
        self.skill_ready = true;
        Ok(())
    }
    pub fn begin(&mut self) -> Result<u64, String> {
        if !self.skill_ready || self.active {
            return Err("Agent is unavailable or busy".into());
        }
        self.turn += 1;
        self.active = true;
        Ok(self.turn)
    }
    pub fn permits(&self, turn: u64, mutation: bool) -> bool {
        self.active
            && self.skill_ready
            && turn == self.turn
            && (!mutation || self.context.is_some())
    }
    pub fn stop(&mut self) {
        self.active = false;
        self.turn += 1;
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn device_first() {
        let mut s = Startup::default();
        assert!(s.choose(Provider::Codex).is_err());
        assert!(s.begin().is_err());
    }
    #[test]
    fn initialization_cannot_mutate() {
        let s = Startup::default();
        assert!(!s.permits(0, true));
    }
    #[test]
    fn no_save_restore_tools() {
        assert!(
            !TOOLS
                .iter()
                .any(|(n, _)| n.contains("save") || n.contains("restore") || n.contains("midi"))
        );
        assert!(SKILL.contains("version: 1"));
    }
}
