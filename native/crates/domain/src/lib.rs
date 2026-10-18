//! Provider- and transport-independent application contracts.
use serde::{Deserialize, Serialize};
use thiserror::Error;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Target {
    Hardware,
    TestMock,
}
impl Target {
    pub fn namespace(self) -> &'static str {
        match self {
            Self::Hardware => "hardware",
            Self::TestMock => "mock",
        }
    }
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Context {
    pub session: String,
    pub generation: u64,
    pub preset: u16,
    pub scene: Option<u8>,
    pub channel: Option<u8>,
    pub target: Target,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Parameter {
    pub id: String,
    pub label: String,
    pub unit: String,
    pub confirmed: Option<f64>,
    /// Device-provided textual value for parameters such as amp model names.
    #[serde(default)]
    pub display_text: Option<String>,
    /// Unconverted float returned by the device; never an editable value by itself.
    #[serde(default)]
    pub raw_numeric: Option<f64>,
    #[serde(default)]
    pub read_status: Option<String>,
    #[serde(default)]
    pub read_error: Option<String>,
    pub range: Option<(f64, f64)>,
    pub writable: bool,
    pub automated: Option<bool>,
    #[serde(default)]
    pub raw_bits: Option<u32>,
}
impl Parameter {
    pub fn validate(&self, value: f64) -> Result<(), Error> {
        if !self.writable || self.automated != Some(false) {
            return Err(Error::Unsupported);
        }
        let (min, max) = self.range.ok_or(Error::Unsupported)?;
        if !min.is_finite()
            || !max.is_finite()
            || min > max
            || !value.is_finite()
            || value < min
            || value > max
        {
            return Err(Error::InvalidValue);
        }
        if !self.confirmed.is_some_and(f64::is_finite) {
            return Err(Error::UnknownOriginal);
        }
        Ok(())
    }
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Block {
    pub id: String,
    pub family: String,
    /// Human-readable effect family when the device instance ID is recognized.
    #[serde(default)]
    pub type_name: Option<String>,
    /// Axe-Fx grid instance identifier. This is not a model identifier.
    #[serde(default)]
    pub instance_id: Option<u16>,
    /// Static editor-symbol prefix used to enumerate known parameters.
    #[serde(default)]
    pub parameter_prefix: Option<String>,
    #[serde(default)]
    pub channel: Option<u8>,
    #[serde(default)]
    pub bypassed: Option<bool>,
    pub label: String,
    pub position: (u8, u8),
    pub parameters: Vec<Parameter>,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Snapshot {
    pub context: Context,
    pub name: String,
    pub blocks: Vec<Block>,
    pub edges: Vec<(String, String)>,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Change {
    pub block: String,
    pub parameter: String,
    pub value: f64,
}
#[derive(Debug, Error, PartialEq)]
pub enum Error {
    #[error("Operation cancelled; completed writes were checked for rollback")]
    Cancelled,
    #[error("Operation is not verified for this target")]
    Unsupported,
    #[error("Device context changed")]
    StaleContext,
    #[error("Original value is unknown")]
    UnknownOriginal,
    #[error("Value is outside its verified range")]
    InvalidValue,
    #[error("Recovery required; inspect device before further changes")]
    RecoveryRequired,
    #[error("Transaction identifier already used")]
    Duplicate,
    #[error("Device disconnected")]
    Disconnected,
    #[error("Readback mismatch")]
    Readback,
    #[error("Persistence failed: {0}")]
    Storage(String),
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn unknown_and_automation_block_changes() {
        let mut p = Parameter {
            id: "gain".into(),
            label: "Gain".into(),
            unit: "".into(),
            confirmed: None,
            display_text: None,
            raw_numeric: None,
            read_status: None,
            read_error: None,
            range: Some((0., 10.)),
            writable: true,
            automated: Some(false),
            raw_bits: None,
        };
        assert_eq!(p.validate(2.), Err(Error::UnknownOriginal));
        p.confirmed = Some(1.);
        assert!(p.validate(2.).is_ok());
        assert_eq!(p.validate(f64::NAN), Err(Error::InvalidValue));
        p.automated = None;
        assert_eq!(p.validate(2.), Err(Error::Unsupported));
    }
}
