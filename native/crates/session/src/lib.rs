//! Sole session authority. Durable intent always precedes device mutation.
use axe_domain::*;
use rusqlite::{Connection, params};
use std::collections::BTreeSet;

pub trait Transport: Send {
    fn snapshot(&mut self) -> Result<Snapshot, Error>;
    fn read(&mut self, block: &str, parameter: &str) -> Result<f64, Error>;
    fn write(&mut self, block: &str, parameter: &str, value: f64) -> Result<(), Error>;
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
         CREATE TABLE IF NOT EXISTS transactions(namespace TEXT,session TEXT,id TEXT,context TEXT,batch TEXT,status TEXT,PRIMARY KEY(namespace,session,id));
         CREATE TABLE IF NOT EXISTS conversation(id INTEGER PRIMARY KEY,namespace TEXT,session TEXT,provider TEXT,role TEXT,body TEXT);
         CREATE TABLE IF NOT EXISTS settings(key TEXT PRIMARY KEY,value TEXT NOT NULL);").map_err(storage)?;
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
            let mut query=self.store.db.prepare("SELECT block,parameter,value FROM originals WHERE namespace=? AND session=? ORDER BY block,parameter").map_err(storage)?;
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
    pub fn recovery_required(&self) -> bool {
        self.recovery
    }
    pub fn apply(&mut self, expected: &Context, id: &str, changes: &[Change]) -> Result<(), Error> {
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
                self.context(expected)?;
                if self.transport.read(&c.block, &c.parameter)? != before[attempted].value {
                    return Err(Error::StaleContext);
                }
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
                range: Some((0., 10.)),
                writable: true,
                automated: Some(false),
            })
            .collect();
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
                        label: "Input".into(),
                        position: (0, 0),
                        parameters: vec![],
                    },
                    Block {
                        id: "amp1".into(),
                        family: "amp".into(),
                        label: "Test amplifier".into(),
                        position: (2, 0),
                        parameters: params,
                    },
                    Block {
                        id: "output1".into(),
                        family: "output".into(),
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
        }
    }
}
impl Transport for Mock {
    fn snapshot(&mut self) -> Result<Snapshot, Error> {
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
