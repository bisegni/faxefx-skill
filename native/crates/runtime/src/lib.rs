//! In-process application authority shared by native controls and scoped agent tools.
use axe_agents::{
    Provider,
    runtime::{AgentEvent, EventSink, HostCall, Scope, ToolHandler, ToolReply},
};
use axe_domain::{Change, Snapshot, Target};
use axe_session::{ChatMessage, Mock, Session, Store};
use serde_json::{Value, json};
use std::{
    collections::BTreeSet,
    path::PathBuf,
    sync::{Arc, Mutex},
};
pub type Shared = Arc<Mutex<Backend>>;
pub struct Backend {
    pub root: PathBuf,
    _lock: std::fs::File,
    pub store: Store,
    pub snapshot: Option<Snapshot>,
    session: Option<Session<Mock>>,
    attached: BTreeSet<String>,
}
impl Backend {
    pub fn open(root: PathBuf) -> Result<Shared, String> {
        std::fs::create_dir_all(&root).map_err(|e| e.to_string())?;
        let lock = std::fs::OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(root.join("application.lock"))
            .map_err(err)?;
        lock.try_lock().map_err(|_| {
            "Axe Studio is already using this application data directory".to_owned()
        })?;
        let store = Store::open(root.join("studio.sqlite3")).map_err(err)?;
        store.interrupt_streams().map_err(err)?;
        Ok(Arc::new(Mutex::new(Self {
            root,
            _lock: lock,
            store,
            snapshot: None,
            session: None,
            attached: BTreeSet::new(),
        })))
    }
    pub fn bind(&mut self, snapshot: Snapshot) -> Result<(), String> {
        self.store
            .set_setting("last_namespace", snapshot.context.target.namespace())
            .map_err(err)?;
        self.attached.clear();
        self.session = if snapshot.context.target == Target::TestMock {
            let path = self.root.join("mock-edit-buffer.json");
            let state = if path.is_file() {
                serde_json::from_slice::<Snapshot>(&std::fs::read(&path).map_err(err)?)
                    .map_err(err)?
            } else {
                snapshot.clone()
            };
            if state.context.target != Target::TestMock {
                return Err("Invalid mock namespace in persisted buffer".into());
            }
            let transport = Mock {
                state,
                state_path: Some(path),
                ..Mock::default()
            };
            Some(
                Session::connect(
                    transport,
                    Store::open(self.root.join("studio.sqlite3")).map_err(err)?,
                )
                .map_err(err)?,
            )
        } else {
            None
        };
        self.snapshot = Some(
            self.session
                .as_ref()
                .map(|s| s.confirmed.clone())
                .unwrap_or(snapshot),
        );
        Ok(())
    }
    pub fn library(&self) -> Result<axe_audio::artifact::Library, String> {
        axe_audio::artifact::Library::open(self.root.join("audio"))
    }
    pub fn dirty(&self) -> bool {
        self.session
            .as_ref()
            .is_some_and(|s| s.dirty().unwrap_or(true))
    }
    pub fn recovery(&self) -> bool {
        self.session
            .as_ref()
            .is_some_and(Session::recovery_required)
    }
    pub fn capabilities(&self) -> Value {
        json!({"target":self.snapshot.as_ref().map(|s|s.context.target),"parameter_batches":self.session.is_some()&&!self.recovery(),"restore_original":self.session.is_some()&&!self.recovery(),"save_tone":self.session.is_some()&&!self.recovery(),"structural_routing":false,"hardware_writes":false,"hardware_save_reason":"Device setter profiles, modifier protection, backup and save/reload have not passed hardware verification","parameter_baseline_is_full_backup":false})
    }
    pub fn adjust(
        &mut self,
        id: &str,
        changes: &[Change],
        permit: impl Fn() -> bool,
    ) -> Result<Snapshot, String> {
        let s = self
            .session
            .as_mut()
            .ok_or("Hardware writes are unavailable until capability and recovery gates pass")?;
        let c = s.confirmed.context.clone();
        let result = s.apply_guarded(&c, id, changes, permit);
        self.snapshot = Some(s.confirmed.clone());
        result.map_err(err)?;
        Ok(s.confirmed.clone())
    }
    pub fn restore(&mut self) -> Result<Snapshot, String> {
        let s = self
            .session
            .as_mut()
            .ok_or("Hardware original restoration is not verified")?;
        let c = s.confirmed.context.clone();
        let result = if s.recovery_required() {
            s.recover_mock_original()
        } else {
            s.restore_original(&c, &format!("restore-{}", uuid::Uuid::new_v4()))
        };
        self.snapshot = Some(s.confirmed.clone());
        result.map_err(err)?;
        Ok(s.confirmed.clone())
    }
    pub fn save(&mut self, slot: u16) -> Result<(), String> {
        self.session
            .as_mut()
            .ok_or("Hardware save/reload semantics are unresolved; Save tone is unavailable")?
            .save_mock(slot)
            .map_err(err)
    }
    pub fn history(&self, namespace: &str) -> Result<Vec<ChatMessage>, String> {
        self.store.messages(namespace).map_err(err)
    }
    pub fn user_message(
        &mut self,
        namespace: &str,
        provider: Provider,
        text: &str,
    ) -> Result<String, String> {
        let id = uuid::Uuid::new_v4().to_string();
        self.store
            .message(
                namespace,
                &ChatMessage {
                    id: id.clone(),
                    provider: format!("{provider:?}"),
                    role: "user".into(),
                    body: text.into(),
                    status: "queued".into(),
                },
                false,
            )
            .map_err(err)?;
        Ok(id)
    }
    pub fn attach(&mut self, id: &str, attach: bool) -> Result<(), String> {
        self.library()?.report(id)?;
        if attach {
            self.attached.insert(id.into());
        } else {
            self.attached.remove(id);
        }
        Ok(())
    }
    pub fn dispatch(&mut self, call: HostCall, scope: Scope) -> ToolReply {
        if self.snapshot.is_none() {
            return Err("No device snapshot is bound to this conversation".into());
        }
        if !scope.valid() {
            return Err("Turn authority revoked".into());
        }
        if !call.arguments.is_object() {
            return Err("Tool arguments must be an object".into());
        }
        match call.name.as_str() {
            "session_inspect" => Ok(
                json!({"snapshot":self.snapshot,"capabilities":self.capabilities(),"attached_analysis":self.attached}),
            ),
            "block_inspect" => {
                let id = call.arguments["block"]
                    .as_str()
                    .ok_or("Block ID required")?;
                self.snapshot
                    .as_ref()
                    .and_then(|s| s.blocks.iter().find(|b| b.id == id))
                    .map(|b| json!(b))
                    .ok_or("Unknown block".into())
            }
            "analysis_attached" => {
                let id = call.arguments["id"]
                    .as_str()
                    .ok_or("Analysis ID required")?;
                if !self.attached.contains(id) {
                    return Err("Only explicitly attached summaries are available".into());
                }
                let report = self.library()?.report(id)?;
                Ok(
                    json!({"measurements":report["measurements"],"excerpt":report["excerpt"],"limitations":report["limitations"]}),
                )
            }
            "parameters_set" => {
                if !scope.writable() {
                    return Err("Initialization or revoked turns cannot mutate".into());
                }
                let id = call.arguments["idempotency_key"]
                    .as_str()
                    .ok_or("Idempotency key required")?;
                let changes: Vec<Change> =
                    serde_json::from_value(call.arguments["changes"].clone())
                        .map_err(|_| "Invalid parameter batch")?;
                self.adjust(id, &changes, || scope.writable())
                    .map(|s| json!({"status":"verified","snapshot":s}))
            }
            "transaction_status" => Ok(
                json!({"dirty":self.dirty(),"recovery_required":self.recovery(),"transactions":self.snapshot.as_ref().map(|s|self.store.transaction_summaries(&s.context)).transpose().map_err(err)?}),
            ),
            _ => Err("Tool not exposed by Axe Studio".into()),
        }
    }
}
fn err(e: impl std::fmt::Display) -> String {
    e.to_string()
}
pub fn handlers(shared: Shared, namespace: String, provider: Provider) -> (ToolHandler, EventSink) {
    let backend = shared.clone();
    let tools: ToolHandler = Arc::new(move |call, scope| {
        backend
            .lock()
            .map_err(|_| "Backend unavailable")?
            .dispatch(call, scope)
    });
    let connection = uuid::Uuid::new_v4().to_string();
    let persist: EventSink = Arc::new(move |event| {
        let mut b = shared.lock().map_err(|_| "Backend unavailable")?;
        let p = format!("{provider:?}");
        b.store
            .event(&namespace, &serde_json::to_string(event).map_err(err)?)
            .map_err(err)?;
        match event {
            AgentEvent::Session { id, .. } => b
                .store
                .set_setting(&format!("provider.{namespace}.{p}"), id)
                .map_err(err),
            AgentEvent::Delta { id, text } => b
                .store
                .message(
                    &namespace,
                    &ChatMessage {
                        id: format!("{connection}/{id}").to_string(),
                        provider: p.to_string(),
                        role: ("assistant").to_string(),
                        body: (text).to_string(),
                        status: ("streaming").to_string(),
                    },
                    true,
                )
                .map_err(err),
            AgentEvent::Message { id, text } => b
                .store
                .message(
                    &namespace,
                    &ChatMessage {
                        id: format!("{connection}/{id}").to_string(),
                        provider: p.to_string(),
                        role: ("assistant").to_string(),
                        body: (text).to_string(),
                        status: ("complete").to_string(),
                    },
                    false,
                )
                .map_err(err),
            AgentEvent::Activity {
                id,
                summary,
                result,
            } => b
                .store
                .message(
                    &namespace,
                    &ChatMessage {
                        id: format!("{connection}/tool/{id}").to_string(),
                        provider: p.to_string(),
                        role: ("tool").to_string(),
                        body: format!("{summary}\n{result}").to_string(),
                        status: ("complete").to_string(),
                    },
                    false,
                )
                .map_err(err),
            AgentEvent::Interrupted | AgentEvent::Failed(_) => {
                b.store.interrupt_streams().map_err(err)
            }
            _ => Ok(()),
        }
    });
    (tools, persist)
}
#[cfg(test)]
mod tests {
    use super::*;
    use axe_session::Transport;
    #[test]
    fn durable_mock_restores_after_restart_without_replaying_changes() {
        let root = std::env::temp_dir().join(format!("axe-durable-mock-{}", uuid::Uuid::new_v4()));
        {
            let shared = Backend::open(root.clone()).unwrap();
            let mut b = shared.lock().unwrap();
            b.bind(Mock::default().snapshot().unwrap()).unwrap();
            b.adjust(
                "durable",
                &[Change {
                    block: "amp1".into(),
                    parameter: "gain".into(),
                    value: 7.,
                }],
                || true,
            )
            .unwrap();
            assert!(b.dirty());
        }
        {
            let shared = Backend::open(root.clone()).unwrap();
            let mut b = shared.lock().unwrap();
            b.bind(Mock::default().snapshot().unwrap()).unwrap();
            assert!(b.dirty());
            let value = b
                .snapshot
                .as_ref()
                .unwrap()
                .blocks
                .iter()
                .find(|b| b.id == "amp1")
                .unwrap()
                .parameters
                .iter()
                .find(|p| p.id == "gain")
                .unwrap()
                .confirmed;
            assert_eq!(value, Some(7.));
            b.restore().unwrap();
            assert!(!b.dirty());
        }
        let _ = std::fs::remove_dir_all(root);
    }
    #[test]
    fn mock_and_hardware_capabilities_separate() {
        let root = std::env::temp_dir().join(format!("axe-runtime-{}", uuid::Uuid::new_v4()));
        let b = Backend::open(root.clone()).unwrap();
        {
            let mut b = b.lock().unwrap();
            let mut s = Mock::default().snapshot().unwrap();
            b.bind(s.clone()).unwrap();
            assert_eq!(b.capabilities()["parameter_batches"], true);
            s.context.target = Target::Hardware;
            b.bind(s).unwrap();
            assert_eq!(b.capabilities()["parameter_batches"], false);
            assert!(!b.dirty());
        }
        drop(b);
        let _ = std::fs::remove_dir_all(root);
    }
}
