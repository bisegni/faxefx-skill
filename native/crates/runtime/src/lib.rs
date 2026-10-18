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
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SaveOutcome {
    MockVerified,
    HardwareSentUnverified { destination: u16 },
}
pub struct Backend {
    pub root: PathBuf,
    _lock: std::fs::File,
    pub store: Store,
    pub snapshot: Option<Snapshot>,
    session: Option<Session<Mock>>,
    hardware_endpoints: Option<(String, String)>,
    attached: BTreeSet<String>,
    midi: Option<axe_session::usb::live::MidiService>,
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
            hardware_endpoints: None,
            attached: BTreeSet::new(),
            midi: None,
        })))
    }
    pub fn bind(&mut self, snapshot: Snapshot) -> Result<(), String> {
        if self
            .midi
            .as_ref()
            .is_some_and(|m| m.live.snapshot.context.session != snapshot.context.session)
        {
            self.midi = None;
        }
        self.hardware_endpoints = None;
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
    pub fn bind_hardware(
        &mut self,
        snapshot: Snapshot,
        input: String,
        output: String,
    ) -> Result<(), String> {
        if snapshot.context.target != Target::Hardware || input.is_empty() || output.is_empty() {
            return Err("Hardware save requires a hardware capture and selected endpoints".into());
        }
        self.bind(snapshot)?;
        self.hardware_endpoints = Some((input, output));
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
        json!({"target":self.snapshot.as_ref().map(|s|s.context.target),"parameter_batches":self.session.is_some()&&!self.recovery(),"restore_original":self.session.is_some()&&!self.recovery(),"save_tone":(self.session.is_some()||self.hardware_endpoints.is_some())&&!self.recovery(),"structural_routing":false,"hardware_writes":self.hardware_endpoints.is_some(),"hardware_parameter_writes":false,"hardware_preset_store":self.hardware_endpoints.is_some(),"hardware_save_verification":"Store is user-operated and sent once after context preflight; persistence requires manual verification","parameter_baseline_is_full_backup":false,"parameter_catalog":axe_session::usb::catalog_info()})
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
    pub fn save(&mut self, slot: u16) -> Result<SaveOutcome, String> {
        if slot > 1023 {
            return Err("Preset destination must be 0–1023".into());
        }
        if let Some(session) = self.session.as_mut() {
            session.save_mock(slot).map_err(err)?;
            return Ok(SaveOutcome::MockVerified);
        }
        let mut midi = self.midi.take().ok_or("Reconnect MIDI before saving")?;
        let result = self.hardware_save(slot, |snapshot, before| midi.save(snapshot, slot, before));
        self.midi = Some(midi);
        result
    }
    pub fn capture_hardware(&mut self, input: String, output: String) -> Result<Snapshot, String> {
        self.midi = None;
        self.snapshot = None;
        self.hardware_endpoints = None;
        let midi = axe_session::usb::live::MidiService::connect(&input, &output)?;
        let snapshot = midi.live.snapshot.clone();
        self.midi = Some(midi);
        self.bind_hardware(snapshot.clone(), input, output)?;
        Ok(snapshot)
    }
    pub fn synchronize(
        &mut self,
        session: &str,
        selected: Option<String>,
        visible: Vec<String>,
    ) -> Result<Snapshot, String> {
        let midi = self.midi.as_mut().ok_or("Live MIDI unavailable")?;
        if midi.live.snapshot.context.session != session {
            return Ok(midi.live.snapshot.clone());
        }
        let result = midi.step(selected, visible);
        self.snapshot = Some(midi.live.snapshot.clone());
        // Failed reads invalidate values in both agent inspection and the UI.
        if let Err(error) = result {
            self.store
                .event(
                    "hardware",
                    &json!({"kind":"live_read_failed","error":error}).to_string(),
                )
                .map_err(err)?;
        }
        Ok(midi.live.snapshot.clone())
    }

    fn hardware_save(
        &self,
        slot: u16,
        send: impl FnOnce(&Snapshot, &mut dyn FnMut() -> Result<(), String>) -> Result<(), String>,
    ) -> Result<SaveOutcome, String> {
        let snapshot = self.snapshot.as_ref().ok_or("No captured preset to save")?;
        if slot > 1023
            || snapshot.context.target != Target::Hardware
            || self.hardware_endpoints.is_none()
            || self.recovery()
        {
            return Err("Hardware preset save is unavailable in this context".into());
        }
        let id = uuid::Uuid::new_v4().to_string();
        let event = |status: &str, detail: Option<&str>| {
            self.store.event("hardware", &json!({"kind":"preset_store","id":id,"context":snapshot.context,"destination":slot,"status":status,"detail":detail,"persistence_verified":false}).to_string()).map_err(err)
        };
        event("prepared", None)?;
        let mut attempted = false;
        let result = send(snapshot, &mut || {
            event("sending", None)?;
            attempted = true;
            Ok(())
        });
        let status = match (&result, attempted) {
            (Ok(()), true) => "sent_unverified",
            (Err(_), true) => "uncertain",
            _ => "not_sent",
        };
        event(status, result.as_ref().err().map(String::as_str)).map_err(|e| {
            format!("Store status could not be journaled; delivery may be uncertain: {e}")
        })?;
        result?;
        if !attempted {
            return Err(
                "Save transport did not record an intent; no verified result is available".into(),
            );
        }
        Ok(SaveOutcome::HardwareSentUnverified { destination: slot })
    }
    pub fn select_scene(&mut self, scene: u8) -> Result<Snapshot, String> {
        if let Some(midi) = self.midi.as_mut() {
            let result = midi.scene(scene);
            self.snapshot = result.as_ref().ok().cloned();
            return result;
        }
        let s = self.session.as_mut().ok_or("Scene selection unavailable")?;
        let context = s.confirmed.context.clone();
        let snapshot = s.select_scene(&context, scene).map_err(err)?;
        self.snapshot = Some(snapshot.clone());
        Ok(snapshot)
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
                json!({"snapshot":self.snapshot,"capabilities":self.capabilities(),"parameter_catalog":axe_session::usb::catalog_info(),"live_freshness":self.midi.as_ref().map(|m| &m.live.freshness),"attached_analysis":self.attached}),
            ),
            "block_inspect" => {
                let id = call.arguments["block"]
                    .as_str()
                    .ok_or("Block ID required")?;
                self.snapshot
                    .as_ref()
                    .and_then(|s| s.blocks.iter().find(|b| b.id == id))
                    .map(|b| {
                        let mut value = json!(b);
                        value["live_freshness"] =
                            json!(self.midi.as_ref().map(|m| &m.live.freshness));
                        value
                    })
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
                json!({"dirty":self.dirty(),"recovery_required":self.recovery(),"transactions":self.snapshot.as_ref().map(|s|self.store.transaction_summaries(&s.context)).transpose().map_err(err)?,"application_events":self.snapshot.as_ref().map(|s|self.store.recent_events(s.context.target.namespace())).transpose().map_err(err)?}),
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
    #[test]
    fn user_save_keeps_mock_verification_separate_from_hardware_transmission() {
        let root = std::env::temp_dir().join(format!("axe-save-{}", uuid::Uuid::new_v4()));
        let shared = Backend::open(root.clone()).unwrap();
        {
            let mut backend = shared.lock().unwrap();
            let mut snapshot = Mock::default().snapshot().unwrap();
            backend.bind(snapshot.clone()).unwrap();
            assert_eq!(backend.save(17).unwrap(), SaveOutcome::MockVerified);
            snapshot.context.target = Target::Hardware;
            backend.bind(snapshot.clone()).unwrap();
            assert_eq!(backend.capabilities()["save_tone"], false);
            assert!(backend.save(17).is_err());
            backend
                .bind_hardware(snapshot, "authored input".into(), "authored output".into())
                .unwrap();
            assert_eq!(backend.capabilities()["save_tone"], true);
            let outcome = backend
                .hardware_save(17, |_, before| {
                    before()?;
                    Ok(())
                })
                .unwrap();
            assert_eq!(
                outcome,
                SaveOutcome::HardwareSentUnverified { destination: 17 }
            );
            let events = backend.store.recent_events("hardware").unwrap();
            let events: Vec<Value> = events
                .iter()
                .map(|v| serde_json::from_str(v).unwrap())
                .collect();
            assert_eq!(events[0]["status"], "sent_unverified");
            assert_eq!(events[1]["status"], "sending");
            assert_eq!(events[2]["status"], "prepared");
            assert_eq!(events[0]["id"], events[2]["id"]);
            assert_eq!(events[0]["persistence_verified"], false);
            assert!(
                backend
                    .hardware_save(1024, |_, _| panic!("invalid slot must not reach wire"))
                    .is_err()
            );
            backend
                .hardware_save(18, |_, before| {
                    before()?;
                    Err("disconnected".into())
                })
                .unwrap_err();
            let events = backend.store.recent_events("hardware").unwrap();
            let last: Value = serde_json::from_str(&events[0]).unwrap();
            assert_eq!(last["status"], "uncertain");
        }
        drop(shared);
        let reopened = Backend::open(root.clone()).unwrap();
        {
            let backend = reopened.lock().unwrap();
            assert!(backend.snapshot.is_none());
            assert_eq!(backend.capabilities()["save_tone"], false);
            assert_eq!(backend.store.recent_events("hardware").unwrap().len(), 6);
        }
        drop(reopened);
        let _ = std::fs::remove_dir_all(root);
    }
    #[test]
    fn scene_selection_updates_mock_snapshot_without_hardware_transport() {
        let root = std::env::temp_dir().join(format!("axe-scene-{}", uuid::Uuid::new_v4()));
        let shared = Backend::open(root.clone()).unwrap();
        {
            let mut backend = shared.lock().unwrap();
            backend.bind(Mock::default().snapshot().unwrap()).unwrap();
            let selected = backend.select_scene(6).unwrap();
            assert_eq!(selected.context.scene, Some(6));
            assert_eq!(selected.scene_names.len(), 8);
            assert_eq!(backend.snapshot.as_ref(), Some(&selected));
        }
        drop(shared);
        let _ = std::fs::remove_dir_all(root);
    }
}
