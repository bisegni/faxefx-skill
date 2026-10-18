//! Normalized provider boundary; availability never implies permission.
use serde::{Deserialize, Serialize};
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Provider {
    Codex,
    Claude,
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
#[cfg(test)]
mod tests {
    use super::*;
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

mod claude;
mod codex;
mod mcp;
mod process;
pub mod runtime;
