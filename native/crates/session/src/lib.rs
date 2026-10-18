//! Sole session authority. Durable intent always precedes device mutation.
use axe_domain::*;
use rusqlite::{Connection, params};
use std::collections::BTreeSet;

pub trait Transport: Send {
    fn snapshot(&mut self) -> Result<Snapshot, Error>;
    fn read(&mut self, block: &str, parameter: &str) -> Result<f64, Error>;
    fn write(&mut self, block: &str, parameter: &str, value: f64) -> Result<(), Error>;
}
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ChatMessage {
    pub id: String,
    pub provider: String,
    pub role: String,
    pub body: String,
    pub status: String,
}
pub struct Store {
    db: Connection,
}
impl Store {
    pub fn open(path: impl AsRef<std::path::Path>) -> Result<Self, Error> {
        Self::from_connection(Connection::open(path).map_err(storage)?)
    }
    pub fn memory() -> Result<Self, Error> {
        Self::from_connection(Connection::open_in_memory().map_err(storage)?)
    }
    fn from_connection(db: Connection) -> Result<Self, Error> {
        db.execute_batch("PRAGMA journal_mode=WAL; PRAGMA synchronous=FULL;
         CREATE TABLE IF NOT EXISTS originals(namespace TEXT,session TEXT,block TEXT,parameter TEXT,value REAL NOT NULL,PRIMARY KEY(namespace,session,block,parameter));
         CREATE TABLE IF NOT EXISTS touches(namespace TEXT,session TEXT,block TEXT,parameter TEXT,PRIMARY KEY(namespace,session,block,parameter));
         CREATE TABLE IF NOT EXISTS transactions(namespace TEXT,session TEXT,id TEXT,context TEXT,batch TEXT,status TEXT,PRIMARY KEY(namespace,session,id));
         CREATE TABLE IF NOT EXISTS conversation(id INTEGER PRIMARY KEY,namespace TEXT,session TEXT,provider TEXT,role TEXT,body TEXT);
         CREATE TABLE IF NOT EXISTS settings(key TEXT PRIMARY KEY,value TEXT NOT NULL);
         CREATE TABLE IF NOT EXISTS messages(seq INTEGER PRIMARY KEY AUTOINCREMENT,id TEXT UNIQUE,namespace TEXT,provider TEXT,role TEXT,body TEXT,status TEXT);
         CREATE TABLE IF NOT EXISTS app_events(seq INTEGER PRIMARY KEY AUTOINCREMENT,namespace TEXT,event TEXT);").map_err(storage)?;
        Ok(Self { db })
    }
    pub fn append_chat(
        &self,
        context: &Context,
        provider: &str,
        role: &str,
        body: &str,
    ) -> Result<i64, Error> {
        self.db
            .execute(
                "INSERT INTO conversation(namespace,session,provider,role,body) VALUES(?,?,?,?,?)",
                params![
                    context.target.namespace(),
                    context.session,
                    provider,
                    role,
                    body
                ],
            )
            .map_err(storage)?;
        Ok(self.db.last_insert_rowid())
    }
    pub fn chat(&self, context: &Context) -> Result<Vec<(i64, String, String, String)>, Error> {
        let mut q=self.db.prepare("SELECT id,provider,role,body FROM conversation WHERE namespace=? AND session=? ORDER BY id").map_err(storage)?;
        q.query_map(params![context.target.namespace(), context.session], |r| {
            Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?))
        })
        .map_err(storage)?
        .collect::<Result<_, _>>()
        .map_err(storage)
    }
    pub fn message(
        &mut self,
        namespace: &str,
        entry: &ChatMessage,
        append: bool,
    ) -> Result<(), Error> {
        let tx = self.db.transaction().map_err(storage)?;
        tx.execute("INSERT INTO messages(id,namespace,provider,role,body,status) VALUES(?,?,?,?,?,?) ON CONFLICT(id) DO UPDATE SET body=CASE WHEN ? THEN messages.body || excluded.body ELSE excluded.body END,status=excluded.status",params![entry.id,namespace,entry.provider,entry.role,entry.body,entry.status,append]).map_err(storage)?;
        tx.commit().map_err(storage)?;
        Ok(())
    }
    pub fn message_status(&self, id: &str, status: &str) -> Result<(), Error> {
        self.db
            .execute(
                "UPDATE messages SET status=? WHERE id=?",
                params![status, id],
            )
            .map_err(storage)?;
        Ok(())
    }
    pub fn transaction_summaries(
        &self,
        context: &Context,
    ) -> Result<Vec<serde_json::Value>, Error> {
        let mut q=self.db.prepare("SELECT id,status FROM transactions WHERE namespace=? AND session=? ORDER BY rowid DESC LIMIT 64").map_err(storage)?;
        q.query_map(params![context.target.namespace(), context.session], |r| {
            Ok(serde_json::json!({"id":r.get::<_,String>(0)?,"status":r.get::<_,String>(1)?}))
        })
        .map_err(storage)?
        .collect::<Result<_, _>>()
        .map_err(storage)
    }
    pub fn messages(&self, namespace: &str) -> Result<Vec<ChatMessage>, Error> {
        let mut q = self
            .db
            .prepare(
                "SELECT id,provider,role,body,status FROM messages WHERE namespace=? ORDER BY seq",
            )
            .map_err(storage)?;
        q.query_map([namespace], |r| {
            Ok(ChatMessage {
                id: r.get(0)?,
                provider: r.get(1)?,
                role: r.get(2)?,
                body: r.get(3)?,
                status: r.get(4)?,
            })
        })
        .map_err(storage)?
        .collect::<Result<_, _>>()
        .map_err(storage)
    }
    pub fn event(&self, namespace: &str, event: &str) -> Result<(), Error> {
        self.db
            .execute(
                "INSERT INTO app_events(namespace,event) VALUES(?,?)",
                params![namespace, event],
            )
            .map_err(storage)?;
        Ok(())
    }
    pub fn interrupt_streams(&self) -> Result<(), Error> {
        self.db
            .execute(
                "UPDATE messages SET status='interrupted' WHERE status='streaming'",
                [],
            )
            .map_err(storage)?;
        Ok(())
    }
    pub fn setting(&self, key: &str) -> Result<Option<String>, Error> {
        use rusqlite::OptionalExtension;
        self.db
            .query_row("SELECT value FROM settings WHERE key=?", [key], |r| {
                r.get(0)
            })
            .optional()
            .map_err(storage)
    }
    pub fn set_setting(&self, key: &str, value: &str) -> Result<(), Error> {
        self.db.execute("INSERT INTO settings VALUES(?,?) ON CONFLICT(key) DO UPDATE SET value=excluded.value",params![key,value]).map_err(storage)?;
        Ok(())
    }
}
fn storage(e: impl std::fmt::Display) -> Error {
    Error::Storage(e.to_string())
}
pub struct Session<T: Transport> {
    transport: T,
    pub store: Store,
    pub confirmed: Snapshot,
    recovery: bool,
}
impl<T: Transport> Session<T> {
    pub fn connect(mut transport: T, store: Store) -> Result<Self, Error> {
        let confirmed = transport.snapshot()?;
        let unresolved:i64=store.db.query_row("SELECT count(*) FROM transactions WHERE namespace=? AND status IN ('pending','recovery')",[confirmed.context.target.namespace()],|r|r.get(0)).map_err(storage)?;
        Ok(Self {
            transport,
            store,
            confirmed,
            recovery: unresolved > 0,
        })
    }
    fn context(&mut self, expected: &Context) -> Result<(), Error> {
        if self.recovery {
            return Err(Error::RecoveryRequired);
        }
        if &self.confirmed.context != expected || &self.transport.snapshot()?.context != expected {
            return Err(Error::StaleContext);
        }
        Ok(())
    }
    /// User command only: restore session-touched parameters, preserving others.
    pub fn restore_original(&mut self, expected: &Context, id: &str) -> Result<(), Error> {
        self.context(expected)?;
        let changes = {
            let mut query=self.store.db.prepare("SELECT o.block,o.parameter,o.value FROM originals o JOIN touches t ON o.namespace=t.namespace AND o.session=t.session AND o.block=t.block AND o.parameter=t.parameter WHERE o.namespace=? AND o.session=? ORDER BY o.block,o.parameter").map_err(storage)?;
            query
                .query_map(
                    params![expected.target.namespace(), expected.session],
                    |r| {
                        Ok(Change {
                            block: r.get(0)?,
                            parameter: r.get(1)?,
                            value: r.get(2)?,
                        })
                    },
                )
                .map_err(storage)?
                .collect::<Result<Vec<_>, _>>()
                .map_err(storage)?
        };
        if changes.is_empty() {
            return Ok(());
        }
        self.apply(expected, id, &changes)
    }
    pub fn dirty(&self) -> Result<bool, Error> {
        let mut q = self
            .store
            .db
            .prepare("SELECT o.block,o.parameter,o.value FROM originals o JOIN touches t ON o.namespace=t.namespace AND o.session=t.session AND o.block=t.block AND o.parameter=t.parameter WHERE o.namespace=? AND o.session=?")
            .map_err(storage)?;
        let rows = q
            .query_map(
                params![
                    self.confirmed.context.target.namespace(),
                    self.confirmed.context.session
                ],
                |r| {
                    Ok((
                        r.get::<_, String>(0)?,
                        r.get::<_, String>(1)?,
                        r.get::<_, f64>(2)?,
                    ))
                },
            )
            .map_err(storage)?;
        for r in rows {
            let (b, p, v) = r.map_err(storage)?;
            let confirmed = self
                .confirmed
                .blocks
                .iter()
                .find(|x| x.id == b)
                .and_then(|b| b.parameters.iter().find(|x| x.id == p))
                .and_then(|p| p.confirmed);
            if confirmed != Some(v) {
                return Ok(true);
            }
        }
        Ok(false)
    }
    pub fn save_mock(&mut self, destination: u16) -> Result<(), Error> {
        if self.confirmed.context.target != Target::TestMock || destination > 1023 {
            return Err(Error::Unsupported);
        }
        if self.recovery {
            return Err(Error::RecoveryRequired);
        }
        let tx = self.store.db.transaction().map_err(storage)?;
        let json = serde_json::to_string(&self.confirmed).map_err(storage)?;
        tx.execute(
            "INSERT INTO settings VALUES(?,?) ON CONFLICT(key) DO UPDATE SET value=excluded.value",
            params![format!("mock.preset.{destination}"), json],
        )
        .map_err(storage)?;
        tx.execute(
            "DELETE FROM originals WHERE namespace=? AND session=?",
            params![
                self.confirmed.context.target.namespace(),
                self.confirmed.context.session
            ],
        )
        .map_err(storage)?;
        tx.execute(
            "DELETE FROM touches WHERE namespace=? AND session=?",
            params![
                self.confirmed.context.target.namespace(),
                self.confirmed.context.session
            ],
        )
        .map_err(storage)?;
        tx.commit().map_err(storage)?;
        Ok(())
    }
    /// Explicit user recovery for the app-owned durable mock only. Never runs on reconnect.
    pub fn recover_mock_original(&mut self) -> Result<(), Error> {
        if self.confirmed.context.target != Target::TestMock {
            return Err(Error::Unsupported);
        }
        let current = self.transport.snapshot()?;
        if current.context != self.confirmed.context {
            return Err(Error::StaleContext);
        }
        self.confirmed = current;
        self.recovery = false;
        let context = self.confirmed.context.clone();
        let id = format!(
            "recovery-{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map_err(storage)?
                .as_nanos()
        );
        if let Err(error) = self.restore_original(&context, &id) {
            self.recovery = true;
            return Err(error);
        }
        self.store.db.execute("UPDATE transactions SET status='resolved_by_explicit_restore' WHERE namespace=? AND session=? AND status IN ('pending','recovery')",params![context.target.namespace(),context.session]).map_err(storage)?;
        let unresolved:i64=self.store.db.query_row("SELECT count(*) FROM transactions WHERE namespace=? AND status IN ('pending','recovery')",[context.target.namespace()],|r|r.get(0)).map_err(storage)?;
        self.recovery = unresolved > 0;
        if self.recovery {
            return Err(Error::RecoveryRequired);
        }
        Ok(())
    }
    pub fn recovery_required(&self) -> bool {
        self.recovery
    }
    pub fn apply(&mut self, expected: &Context, id: &str, changes: &[Change]) -> Result<(), Error> {
        self.apply_guarded(expected, id, changes, || true)
    }
    pub fn apply_guarded(
        &mut self,
        expected: &Context,
        id: &str,
        changes: &[Change],
        permit: impl Fn() -> bool,
    ) -> Result<(), Error> {
        if !permit() {
            return Err(Error::Cancelled);
        }
        self.context(expected)?;
        // Physical setter authorization/raw-value profiles have not passed their gate.
        if expected.target == Target::Hardware {
            return Err(Error::Unsupported);
        }
        if changes.is_empty() || changes.len() > 64 || id.is_empty() || id.len() > 128 {
            return Err(Error::InvalidValue);
        }
        let mut seen = BTreeSet::new();
        let mut before = Vec::new();
        for c in changes {
            if !seen.insert((&c.block, &c.parameter)) {
                return Err(Error::InvalidValue);
            }
            let p = self
                .confirmed
                .blocks
                .iter()
                .find(|b| b.id == c.block)
                .and_then(|b| b.parameters.iter().find(|p| p.id == c.parameter))
                .ok_or(Error::Unsupported)?;
            p.validate(c.value)?;
            let value = self.transport.read(&c.block, &c.parameter)?;
            if !value.is_finite() {
                return Err(Error::UnknownOriginal);
            }
            if Some(value) != p.confirmed {
                return Err(Error::StaleContext);
            }
            before.push(Change { value, ..c.clone() });
        }
        self.context(expected)?;
        let tx = self.store.db.transaction().map_err(storage)?;
        let exists: i64 = tx
            .query_row(
                "SELECT count(*) FROM transactions WHERE namespace=? AND session=? AND id=?",
                params![expected.target.namespace(), expected.session, id],
                |r| r.get(0),
            )
            .map_err(storage)?;
        if exists > 0 {
            return Err(Error::Duplicate);
        }
        for c in &before {
            tx.execute(
                "INSERT OR IGNORE INTO originals VALUES(?,?,?,?,?)",
                params![
                    expected.target.namespace(),
                    expected.session,
                    c.block,
                    c.parameter,
                    c.value
                ],
            )
            .map_err(storage)?;
        }
        tx.execute(
            "INSERT INTO transactions VALUES(?,?,?,?,?, 'pending')",
            params![
                expected.target.namespace(),
                expected.session,
                id,
                serde_json::to_string(expected).map_err(storage)?,
                serde_json::to_string(&serde_json::json!({"requested":changes,"before":before}))
                    .map_err(storage)?
            ],
        )
        .map_err(storage)?;
        tx.commit().map_err(storage)?;
        let mut attempted = 0;
        let result = (|| {
            for c in changes {
                if !permit() {
                    return Err(Error::Cancelled);
                }
                self.context(expected)?;
                if self.transport.read(&c.block, &c.parameter)? != before[attempted].value {
                    return Err(Error::StaleContext);
                }
                self.store
                    .db
                    .execute(
                        "INSERT OR IGNORE INTO touches VALUES(?,?,?,?)",
                        params![
                            expected.target.namespace(),
                            expected.session,
                            c.block,
                            c.parameter
                        ],
                    )
                    .map_err(storage)?;
                attempted += 1;
                self.transport.write(&c.block, &c.parameter, c.value)?;
                if self.transport.read(&c.block, &c.parameter)? != c.value {
                    return Err(Error::Readback);
                }
            }
            self.context(expected)?;
            Ok(())
        })();
        let status = if result.is_ok() {
            "verified"
        } else {
            let rollback = (|| {
                for c in before[..attempted].iter().rev() {
                    self.context(expected)?;
                    self.transport.write(&c.block, &c.parameter, c.value)?;
                    if self.transport.read(&c.block, &c.parameter)? != c.value {
                        return Err(Error::Readback);
                    }
                }
                Ok::<_, Error>(())
            })();
            if rollback.is_ok() {
                "rolled_back"
            } else {
                self.recovery = true;
                "recovery"
            }
        };
        if let Err(e) = self.store.db.execute(
            "UPDATE transactions SET status=? WHERE namespace=? AND session=? AND id=?",
            params![status, expected.target.namespace(), expected.session, id],
        ) {
            self.recovery = true;
            return Err(storage(e));
        }
        if self.recovery {
            return Err(Error::RecoveryRequired);
        }
        // A new preset/context must be captured as a new session, not adopted
        // into the old original-value baseline after a failed transaction.
        if result == Err(Error::StaleContext) {
            return result;
        }
        match self.transport.snapshot() {
            Ok(snapshot) if snapshot.context == *expected => self.confirmed = snapshot,
            _ => {
                self.recovery = true;
                self.store.db.execute("UPDATE transactions SET status='recovery' WHERE namespace=? AND session=? AND id=?",params![expected.target.namespace(),expected.session,id]).map_err(storage)?;
                return Err(Error::RecoveryRequired);
            }
        }
        result
    }
}
/// Explicit simulation: this type has no MIDI dependency or endpoint access.
pub struct Mock {
    pub state: Snapshot,
    pub fail_write: Option<usize>,
    pub writes: usize,
    pub state_path: Option<std::path::PathBuf>,
}
impl Default for Mock {
    fn default() -> Self {
        let params = ["gain", "bass", "mid", "treble", "master", "presence"]
            .into_iter()
            .map(|id| Parameter {
                id: id.into(),
                label: id.into(),
                unit: "mock units".into(),
                confirmed: Some(5.),
                display_text: None,
                range: Some((0., 10.)),
                writable: true,
                automated: Some(false),
                raw_bits: None,
            })
            .collect();
        let mut params: Vec<Parameter> = params;
        params.insert(
            0,
            Parameter {
                id: "model".into(),
                label: "Model".into(),
                unit: "mock text".into(),
                confirmed: None,
                display_text: Some("Test model".into()),
                range: None,
                writable: false,
                automated: None,
                raw_bits: None,
            },
        );
        params.push(Parameter {
            id: "level".into(),
            label: "Level".into(),
            unit: "mock text".into(),
            confirmed: None,
            display_text: Some("0 dB".into()),
            range: None,
            writable: false,
            automated: None,
            raw_bits: None,
        });
        Self {
            state: Snapshot {
                context: Context {
                    session: format!(
                        "test-mock-{}",
                        std::time::SystemTime::now()
                            .duration_since(std::time::UNIX_EPOCH)
                            .expect("system clock")
                            .as_nanos()
                    ),
                    generation: 1,
                    preset: 0,
                    scene: Some(0),
                    channel: Some(0),
                    target: Target::TestMock,
                },
                name: "Test mock — authored routing".into(),
                blocks: vec![
                    Block {
                        id: "input1".into(),
                        family: "input".into(),
                        type_name: Some("Input 1".into()),
                        instance_id: None,
                        label: "Input".into(),
                        position: (0, 0),
                        parameters: vec![],
                    },
                    Block {
                        id: "amp1".into(),
                        family: "amp".into(),
                        type_name: Some("Amplifier".into()),
                        instance_id: Some(58),
                        label: "Test amplifier".into(),
                        position: (2, 0),
                        parameters: params,
                    },
                    Block {
                        id: "output1".into(),
                        family: "output".into(),
                        type_name: Some("Output 1".into()),
                        instance_id: None,
                        label: "Output".into(),
                        position: (4, 0),
                        parameters: vec![],
                    },
                ],
                edges: vec![
                    ("input1".into(), "amp1".into()),
                    ("amp1".into(), "output1".into()),
                ],
            },
            fail_write: None,
            writes: 0,
            state_path: None,
        }
    }
}
impl Transport for Mock {
    fn snapshot(&mut self) -> Result<Snapshot, Error> {
        if let Some(path) = &self.state_path
            && path.is_file()
        {
            let state: Snapshot =
                serde_json::from_slice(&std::fs::read(path).map_err(storage)?).map_err(storage)?;
            if state.context.target != Target::TestMock {
                return Err(Error::Unsupported);
            }
            self.state = state;
        }
        Ok(self.state.clone())
    }
    fn read(&mut self, b: &str, p: &str) -> Result<f64, Error> {
        self.state
            .blocks
            .iter()
            .find(|x| x.id == b)
            .and_then(|b| b.parameters.iter().find(|x| x.id == p))
            .and_then(|p| p.confirmed)
            .ok_or(Error::UnknownOriginal)
    }
    fn write(&mut self, b: &str, p: &str, v: f64) -> Result<(), Error> {
        self.writes += 1;
        if self.fail_write == Some(self.writes) {
            return Err(Error::Disconnected);
        }
        let p = self
            .state
            .blocks
            .iter_mut()
            .find(|x| x.id == b)
            .and_then(|b| b.parameters.iter_mut().find(|x| x.id == p))
            .ok_or(Error::Unsupported)?;
        p.confirmed = Some(v);
        if let Some(path) = &self.state_path {
            use std::io::Write;
            let temp = path.with_extension("partial");
            let mut file = std::fs::File::create(&temp).map_err(storage)?;
            file.write_all(&serde_json::to_vec(&self.state).map_err(storage)?)
                .map_err(storage)?;
            file.sync_all().map_err(storage)?;
            std::fs::rename(temp, path).map_err(storage)?;
        }
        Ok(())
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    fn changes() -> Vec<Change> {
        vec![
            Change {
                block: "amp1".into(),
                parameter: "gain".into(),
                value: 7.,
            },
            Change {
                block: "amp1".into(),
                parameter: "bass".into(),
                value: 6.,
            },
        ]
    }
    #[test]
    fn verified_and_deduplicated() {
        let mut s = Session::connect(Mock::default(), Store::memory().unwrap()).unwrap();
        let c = s.confirmed.context.clone();
        s.apply(&c, "1", &changes()).unwrap();
        assert_eq!(s.transport.read("amp1", "gain").unwrap(), 7.);
        assert_eq!(s.apply(&c, "1", &changes()), Err(Error::Duplicate));
    }
    #[test]
    fn partial_failure_rolls_back() {
        let m = Mock {
            fail_write: Some(2),
            ..Mock::default()
        };
        let mut s = Session::connect(m, Store::memory().unwrap()).unwrap();
        let c = s.confirmed.context.clone();
        assert_eq!(s.apply(&c, "1", &changes()), Err(Error::Disconnected));
        assert_eq!(s.transport.read("amp1", "gain").unwrap(), 5.);
        assert_eq!(s.transport.read("amp1", "bass").unwrap(), 5.);
    }
    #[test]
    fn stale_context_sends_nothing() {
        let mut s = Session::connect(Mock::default(), Store::memory().unwrap()).unwrap();
        let mut c = s.confirmed.context.clone();
        c.generation += 1;
        assert_eq!(s.apply(&c, "1", &changes()), Err(Error::StaleContext));
        assert_eq!(s.transport.writes, 0);
    }
    #[test]
    fn restores_original_across_multiple_batches() {
        let mut s = Session::connect(Mock::default(), Store::memory().unwrap()).unwrap();
        let c = s.confirmed.context.clone();
        s.apply(&c, "first", &changes()).unwrap();
        let mut second = changes();
        second[0].value = 8.;
        s.apply(&c, "second", &second).unwrap();
        s.restore_original(&c, "restore").unwrap();
        assert_eq!(s.transport.read("amp1", "gain").unwrap(), 5.);
        assert_eq!(s.transport.read("amp1", "presence").unwrap(), 5.);
    }
    #[test]
    fn rollback_failure_blocks_following_writes() {
        struct Broken {
            inner: Mock,
        }
        impl Transport for Broken {
            fn snapshot(&mut self) -> Result<Snapshot, Error> {
                self.inner.snapshot()
            }
            fn read(&mut self, b: &str, p: &str) -> Result<f64, Error> {
                self.inner.read(b, p)
            }
            fn write(&mut self, b: &str, p: &str, v: f64) -> Result<(), Error> {
                if self.inner.writes >= 1 {
                    return Err(Error::Disconnected);
                }
                self.inner.write(b, p, v)
            }
        }
        let mut s = Session::connect(
            Broken {
                inner: Mock::default(),
            },
            Store::memory().unwrap(),
        )
        .unwrap();
        let c = s.confirmed.context.clone();
        assert_eq!(
            s.apply(&c, "first", &changes()),
            Err(Error::RecoveryRequired)
        );
        assert_eq!(
            s.apply(&c, "next", &changes()),
            Err(Error::RecoveryRequired)
        );
        assert_eq!(s.transport.inner.writes, 1);
    }
    #[test]
    fn restart_does_not_replay_pending_transactions() {
        let store = Store::memory().unwrap();
        store
            .db
            .execute(
                "INSERT INTO transactions VALUES('mock','old','pending-id','{}','{}','pending')",
                [],
            )
            .unwrap();
        let s = Session::connect(Mock::default(), store).unwrap();
        assert!(s.recovery_required());
        assert_eq!(s.transport.writes, 0);
    }
    #[test]
    fn durable_conversation_survives_reopen() {
        let path =
            std::env::temp_dir().join(format!("axe-storage-test-{}.sqlite3", std::process::id()));
        let c = Mock::default().state.context;
        {
            let store = Store::open(&path).unwrap();
            store
                .append_chat(
                    &c,
                    "claude",
                    "assistant.partial",
                    "retained interrupted text",
                )
                .unwrap();
        }
        {
            let store = Store::open(&path).unwrap();
            let chat = store.chat(&c).unwrap();
            assert_eq!(chat.len(), 1);
            assert_eq!(chat[0].3, "retained interrupted text");
        }
        let _ = std::fs::remove_file(path);
    }
    #[test]
    fn cancellation_between_writes_rolls_back_and_never_touches_later_targets() {
        use std::cell::Cell;
        let n = Cell::new(0);
        let mut s = Session::connect(Mock::default(), Store::memory().unwrap()).unwrap();
        let c = s.confirmed.context.clone();
        let result = s.apply_guarded(&c, "cancelled", &changes(), || {
            let value = n.get();
            n.set(value + 1);
            value < 2
        });
        assert_eq!(result, Err(Error::Cancelled));
        assert_eq!(s.transport.read("amp1", "gain").unwrap(), 5.);
        assert_eq!(s.transport.writes, 2);
        let touched: i64 = s
            .store
            .db
            .query_row(
                "SELECT count(*) FROM touches WHERE parameter='bass'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(touched, 0);
        assert!(!s.dirty().unwrap());
    }
    #[test]
    fn saved_mock_starts_a_new_original_baseline() {
        let mut s = Session::connect(Mock::default(), Store::memory().unwrap()).unwrap();
        let c = s.confirmed.context.clone();
        s.apply(&c, "first", &changes()).unwrap();
        s.save_mock(17).unwrap();
        assert!(!s.dirty().unwrap());
        let mut changes = changes();
        changes[0].value = 8.;
        s.apply(&c, "next", &changes).unwrap();
        s.restore_original(&c, "restore").unwrap();
        assert_eq!(s.transport.read("amp1", "gain").unwrap(), 7.);
    }
    #[test]
    fn transcript_stream_checkpoint_is_stable_and_interruptible() {
        let mut store = Store::memory().unwrap();
        store
            .message(
                "mock",
                &ChatMessage {
                    id: ("message").to_string(),
                    provider: ("Codex").to_string(),
                    role: ("assistant").to_string(),
                    body: ("part").to_string(),
                    status: ("streaming").to_string(),
                },
                true,
            )
            .unwrap();
        store
            .message(
                "mock",
                &ChatMessage {
                    id: ("message").to_string(),
                    provider: ("Codex").to_string(),
                    role: ("assistant").to_string(),
                    body: ("ial").to_string(),
                    status: ("streaming").to_string(),
                },
                true,
            )
            .unwrap();
        store.interrupt_streams().unwrap();
        let messages = store.messages("mock").unwrap();
        assert_eq!(messages.len(), 1);
        assert_eq!(messages[0].body, "partial");
        assert_eq!(messages[0].status, "interrupted");
        assert!(store.messages("hardware").unwrap().is_empty());
    }
    #[test]
    fn external_scene_change_blocks_rollback_and_all_later_writes() {
        struct Switch(Mock);
        impl Transport for Switch {
            fn snapshot(&mut self) -> Result<Snapshot, Error> {
                if self.0.writes > 0 {
                    self.0.state.context.scene = Some(1);
                }
                self.0.snapshot()
            }
            fn read(&mut self, b: &str, p: &str) -> Result<f64, Error> {
                self.0.read(b, p)
            }
            fn write(&mut self, b: &str, p: &str, v: f64) -> Result<(), Error> {
                self.0.write(b, p, v)
            }
        }
        let mut s = Session::connect(Switch(Mock::default()), Store::memory().unwrap()).unwrap();
        let c = s.confirmed.context.clone();
        assert_eq!(
            s.apply(&c, "external", &changes()),
            Err(Error::RecoveryRequired)
        );
        assert_eq!(s.transport.0.writes, 1);
        assert_eq!(
            s.apply(&c, "later", &changes()),
            Err(Error::RecoveryRequired)
        );
        assert_eq!(
            s.store.transaction_summaries(&c).unwrap()[0]["status"],
            "recovery"
        );
    }
    #[test]
    fn timeout_after_send_is_not_success_and_restores_pre_batch() {
        struct Timeout(Mock);
        impl Transport for Timeout {
            fn snapshot(&mut self) -> Result<Snapshot, Error> {
                self.0.snapshot()
            }
            fn read(&mut self, b: &str, p: &str) -> Result<f64, Error> {
                self.0.read(b, p)
            }
            fn write(&mut self, b: &str, p: &str, v: f64) -> Result<(), Error> {
                self.0.write(b, p, v)?;
                if self.0.writes == 1 {
                    Err(Error::Disconnected)
                } else {
                    Ok(())
                }
            }
        }
        let mut s = Session::connect(Timeout(Mock::default()), Store::memory().unwrap()).unwrap();
        let c = s.confirmed.context.clone();
        assert_eq!(s.apply(&c, "timeout", &changes()), Err(Error::Disconnected));
        assert_eq!(s.transport.0.read("amp1", "gain").unwrap(), 5.);
        assert_eq!(
            s.store.transaction_summaries(&c).unwrap()[0]["status"],
            "rolled_back"
        );
    }
    #[test]
    fn namespace_separates_chat() {
        let store = Store::memory().unwrap();
        let c = Mock::default().state.context;
        store.append_chat(&c, "codex", "user", "hello").unwrap();
        assert_eq!(store.chat(&c).unwrap().len(), 1);
        let h = Context {
            target: Target::Hardware,
            ..c
        };
        assert!(store.chat(&h).unwrap().is_empty());
    }
}

pub mod protocol;

pub mod usb;
