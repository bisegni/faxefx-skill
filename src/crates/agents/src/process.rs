//! Bounded stdio JSON-RPC child; credentials never enter events/transcripts.
use serde_json::{Value, json};
use std::{
    collections::BTreeMap,
    io::{BufRead, BufReader, Read, Write},
    process::{Child, Command, Stdio},
    sync::{Arc, Mutex, mpsc},
    time::Duration,
};
pub const MAX_FRAME: usize = 4 * 1024 * 1024;
pub struct Rpc {
    child: Child,
    input: Arc<Mutex<Option<std::process::ChildStdin>>>,
    pending: Arc<Mutex<BTreeMap<u64, mpsc::SyncSender<Value>>>>,
    pub events: mpsc::Receiver<Value>,
    next: u64,
}
impl Rpc {
    pub fn spawn(command: &mut Command) -> Result<Self, String> {
        let mut child = command
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .map_err(|e| format!("Cannot launch provider: {e}"))?;
        let input = Arc::new(Mutex::new(Some(
            child.stdin.take().ok_or("Provider stdin unavailable")?,
        )));
        let stdout = child.stdout.take().ok_or("Provider stdout unavailable")?;
        let pending = Arc::new(Mutex::new(BTreeMap::<u64, mpsc::SyncSender<Value>>::new()));
        let replies = pending.clone();
        let (tx, events) = mpsc::sync_channel(512);
        std::thread::spawn(move || {
            let mut r = BufReader::new(stdout);
            loop {
                let mut b = vec![];
                match (&mut r)
                    .take((MAX_FRAME + 1) as u64)
                    .read_until(b'\n', &mut b)
                {
                    Ok(0) => break,
                    Ok(_) if b.len() <= MAX_FRAME => {}
                    _ => break,
                }
                let Ok(v) = serde_json::from_slice::<Value>(&b) else {
                    break;
                };
                if v.get("method").is_none()
                    && let Some(id) = v["id"].as_u64()
                {
                    if let Some(reply) = replies.lock().unwrap().remove(&id) {
                        let _ = reply.try_send(v);
                    }
                } else if tx.send(v).is_err() {
                    break;
                }
            }
            for (_, p) in std::mem::take(&mut *replies.lock().unwrap()) {
                let _ = p.try_send(json!({"error":{"message":"Provider disconnected"}}));
            }
            let _ = tx.send(json!({"method":"host/disconnected"}));
        });
        Ok(Self {
            child,
            input,
            pending,
            events,
            next: 0,
        })
    }
    pub fn send(&self, v: &Value) -> Result<(), String> {
        let mut b = serde_json::to_vec(v).map_err(|e| e.to_string())?;
        if b.len() > MAX_FRAME {
            return Err("Provider frame too large".into());
        }
        b.push(b'\n');
        let mut i = self.input.lock().map_err(|_| "Provider pipe failed")?;
        i.as_mut()
            .ok_or("Provider input closed")?
            .write_all(&b)
            .and_then(|_| i.as_mut().unwrap().flush())
            .map_err(|e| e.to_string())
    }
    pub fn write_text(&self, text: &str) -> Result<(), String> {
        if text.len() > 65536 {
            return Err("Prompt too large".into());
        }
        let mut i = self
            .input
            .lock()
            .map_err(|_| "Provider pipe failed")?
            .take()
            .ok_or("Provider input closed")?;
        i.write_all(text.as_bytes())
            .and_then(|_| i.flush())
            .map_err(|e| e.to_string())
    }
    pub fn request(&mut self, method: &str, params: Value) -> Result<Value, String> {
        self.next += 1;
        let id = self.next;
        let (tx, rx) = mpsc::sync_channel(1);
        self.pending.lock().unwrap().insert(id, tx);
        if let Err(e) = self.send(&json!({"id":id,"method":method,"params":params})) {
            self.pending.lock().unwrap().remove(&id);
            return Err(e);
        }
        let response = rx.recv_timeout(Duration::from_secs(30));
        self.pending.lock().unwrap().remove(&id);
        let response = response.map_err(|_| "Provider request timed out".to_string())?;
        if response.get("error").is_some() {
            return Err(response["error"]["message"]
                .as_str()
                .unwrap_or("Provider RPC failed")
                .to_owned());
        }
        Ok(response["result"].clone())
    }
}
impl Drop for Rpc {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}
