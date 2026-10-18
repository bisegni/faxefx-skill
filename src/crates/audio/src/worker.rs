//! Private, bounded, supervised audio-only subprocess.
use serde_json::{Value, json};
use std::{
    io::{BufRead, BufReader, Write},
    path::{Path, PathBuf},
    process::{Child, Command, Stdio},
    sync::mpsc,
    time::Duration,
};
pub struct Worker {
    child: Child,
    input: std::process::ChildStdin,
    output: mpsc::Receiver<Result<Value, String>>,
    next: u64,
}
impl Worker {
    pub fn bundled_path() -> Result<PathBuf, String> {
        if let Some(p) = std::env::var_os("AXE_AUDIO_WORKER") {
            return Ok(p.into());
        }
        let exe = std::env::current_exe().map_err(|e| e.to_string())?;
        let base = exe.parent().ok_or("Application path unavailable")?;
        let name = if cfg!(windows) {
            "axe-audio-worker.exe"
        } else {
            "axe-audio-worker"
        };
        let candidates = [
            base.join("../Resources/audio-worker/axe-audio-worker")
                .join(name),
            base.join("resources/audio-worker/axe-audio-worker")
                .join(name),
        ];
        candidates
            .into_iter()
            .find(|p| p.is_file())
            .ok_or("Bundled audio worker missing; reinstall the application".into())
    }
    pub fn start(path: &Path) -> Result<Self, String> {
        let mut child = Command::new(path)
            .env_remove("PYTHONHOME")
            .env_remove("PYTHONPATH")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .map_err(|e| format!("Cannot start audio worker: {e}"))?;
        let input = child.stdin.take().ok_or("Audio pipe missing")?;
        let stdout = child.stdout.take().ok_or("Audio pipe missing")?;
        let (tx, rx) = mpsc::sync_channel(8);
        std::thread::spawn(move || {
            let mut reader = BufReader::new(stdout);
            loop {
                let mut bytes = vec![];
                let result = std::io::Read::take(&mut reader, (crate::MAX_FRAME + 1) as u64)
                    .read_until(b'\n', &mut bytes);
                match result {
                    Ok(0) => break,
                    Ok(_) if bytes.len() <= crate::MAX_FRAME => {
                        let value = serde_json::from_slice(&bytes)
                            .map_err(|_| "Malformed audio reply".into());
                        if tx.send(value).is_err() {
                            break;
                        }
                    }
                    _ => {
                        let _ = tx.send(Err("Audio frame limit exceeded".into()));
                        break;
                    }
                }
            }
        });
        let mut s = Self {
            child,
            input,
            output: rx,
            next: 0,
        };
        let init = s.request(
            "initialize",
            json!({"protocol_version":crate::PROTOCOL_VERSION}),
        )?;
        if init["protocol_version"] != crate::PROTOCOL_VERSION {
            return Err("Audio protocol mismatch".into());
        }
        Ok(s)
    }
    pub fn request(&mut self, method: &str, params: Value) -> Result<Value, String> {
        self.next += 1;
        let id = self.next;
        let request = crate::Request {
            jsonrpc: "2.0".into(),
            id,
            method: method.into(),
            params,
        };
        let bytes = crate::encode(&request)?;
        self.input
            .write_all(&bytes)
            .and_then(|_| self.input.flush())
            .map_err(|e| e.to_string())?;
        let response = match self.output.recv_timeout(Duration::from_secs(180)) {
            Ok(v) => v?,
            Err(_) => {
                let _ = self.child.kill();
                return Err("Audio worker timed out or disconnected".into());
            }
        };
        if response["id"] != id || response["jsonrpc"] != "2.0" {
            return Err("Audio reply identity mismatch".into());
        }
        if response.get("error").is_some() {
            return Err(
                "Audio analysis failed; use a finite excerpt of at least 0.25 seconds".into(),
            );
        }
        response
            .get("result")
            .cloned()
            .ok_or("Missing audio result".into())
    }
}
impl Drop for Worker {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}
