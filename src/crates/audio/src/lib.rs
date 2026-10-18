//! Audio-only worker protocol. No dependency on session/device or agent tools.
use serde::{Deserialize, Serialize};
pub const PROTOCOL_VERSION: u32 = 1;
pub const MAX_FRAME: usize = 4 * 1024 * 1024;
#[derive(Debug, Serialize, Deserialize)]
pub struct Excerpt {
    pub artifact: String,
    pub start: f64,
    pub duration: f64,
}
impl Excerpt {
    pub fn validate(&self) -> bool {
        self.start.is_finite()
            && self.start >= 0.
            && self.duration.is_finite()
            && self.duration > 0.
            && self.duration <= 120.
            && !self.artifact.is_empty()
    }
}
#[derive(Debug, Serialize, Deserialize)]
pub struct Request {
    pub jsonrpc: String,
    pub id: u64,
    pub method: String,
    pub params: serde_json::Value,
}
pub fn encode(request: &Request) -> Result<Vec<u8>, String> {
    if request.jsonrpc != "2.0"
        || !["initialize", "measure", "shutdown"].contains(&request.method.as_str())
    {
        return Err("Unsupported audio worker method".into());
    }
    let mut bytes = serde_json::to_vec(request).map_err(|e| e.to_string())?;
    if bytes.len() > MAX_FRAME {
        return Err("Frame too large".into());
    }
    bytes.push(b'\n');
    Ok(bytes)
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn rejects_device_methods() {
        let r = Request {
            jsonrpc: "2.0".into(),
            id: 1,
            method: "midi_write".into(),
            params: serde_json::Value::Null,
        };
        assert!(encode(&r).is_err());
    }
    #[test]
    fn bounds() {
        assert!(
            !Excerpt {
                artifact: "x".into(),
                start: 0.,
                duration: 121.
            }
            .validate()
        );
    }
}

pub mod artifact;
pub mod playback;
pub mod worker;
