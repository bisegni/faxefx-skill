//! User-operated preset store. No retry or automatic reload is performed.
use super::*;
use crate::protocol::preset_store;

/// Injectable wire boundary. Production connects MIDI; tests never do.
pub trait StoreWire {
    fn query(&mut self, packet: &[u8]) -> Result<Vec<u8>, String>;
    fn send_once(&mut self, packet: &[u8]) -> Result<(), String>;
    fn settle(&mut self) -> Result<(), String>;
}

fn validate_grid(expected: &Snapshot, grid: &Grid, scene: u8) -> Result<(), String> {
    let live = super::snapshot(grid.clone(), scene)?;
    let placed = |s: &Snapshot| {
        s.blocks
            .iter()
            .filter_map(|b| b.instance_id.map(|id| (b.position, id)))
            .collect::<BTreeMap<_, _>>()
    };
    if grid.slot != expected.context.preset
        || grid.name != expected.name
        || expected.context.scene != Some(scene)
        || placed(&live) != placed(expected)
        || live.edges != expected.edges
    {
        return Err(
            "Preset, scene or routing changed. Read USB again before saving; no store was sent."
                .into(),
        );
    }
    Ok(())
}

fn validate_status(expected: &Snapshot, packet: &[u8]) -> Result<(), String> {
    let (op, payload) = unframe(packet)?;
    if op != 0x13 || !payload.len().is_multiple_of(3) {
        return Err("Invalid block status before saving; no store was sent".into());
    }
    let mut status = BTreeMap::new();
    for entry in payload.as_chunks::<3>().0 {
        let instance = entry[0] as u16 | ((entry[1] as u16) << 7);
        let channel = entry[2] >> 1 & 7;
        let count = entry[2] >> 4 & 7;
        if count > 4 || channel >= count.max(1) {
            return Err("Invalid channel status before saving; no store was sent".into());
        }
        if status
            .insert(instance, (entry[2] >> 1 & 7, entry[2] & 1 != 0))
            .is_some()
        {
            return Err("Duplicate block status before saving; no store was sent".into());
        }
    }
    for block in &expected.blocks {
        if let Some(instance) = block.instance_id
            && (block.channel.is_some() || block.bypassed.is_some())
        {
            let state = status
                .get(&instance)
                .ok_or("Block status changed; read USB again before saving")?;
            if block.channel.is_some_and(|v| v != state.0)
                || block.bypassed.is_some_and(|v| v != state.1)
            {
                return Err("Block channel or bypass changed; read USB again before saving".into());
            }
        }
    }
    Ok(())
}

/// Preflight, durable intent callback, one store, then a bounded settling window.
/// Success means transmission only. It MUST NOT be presented as verified storage.
pub fn store_preset_with(
    wire: &mut impl StoreWire,
    expected: &Snapshot,
    destination: u16,
    durable_intent: impl FnOnce() -> Result<(), String>,
) -> Result<(), String> {
    let packet = preset_store(destination)?;
    if expected.context.target != Target::Hardware || expected.context.scene.is_none() {
        return Err("A captured hardware preset and scene are required for device saving".into());
    }
    let first = decode_grid(&wire.query(&grid_query())?)?;
    let scene = read_scene(&wire.query(&frame(0x0c, &[127])?)?)?;
    validate_grid(expected, &first, scene)?;
    let status = wire.query(&frame(0x13, &[])?)?;
    validate_status(expected, &status)?;
    let second = decode_grid(&wire.query(&grid_query())?)?;
    let second_scene = read_scene(&wire.query(&frame(0x0c, &[127])?)?)?;
    let second_status = wire.query(&frame(0x13, &[])?)?;
    validate_grid(expected, &second, second_scene)?;
    validate_status(expected, &second_status)?;
    if first.cells != second.cells || status != second_status {
        return Err("Device context changed during save preflight; no store was sent".into());
    }
    durable_intent()?;
    wire.send_once(&packet).map_err(|e| format!("Store delivery is uncertain: {e}. Do not automatically retry; verify the destination on the Axe-Fx."))?;
    wire.settle()
        .map_err(|e| format!("Store request was sent; completion is unverified: {e}"))
}

pub(super) trait PacketIo: Send {
    fn send(&mut self, packet: &[u8]) -> Result<(), String>;
    fn receive(&mut self, timeout: Duration) -> Result<Vec<u8>, mpsc::RecvTimeoutError>;
    fn drain(&mut self);
}
struct MidiPackets {
    _input: midir::MidiInputConnection<()>,
    output: midir::MidiOutputConnection,
    replies: mpsc::Receiver<Vec<u8>>,
}
impl PacketIo for MidiPackets {
    fn send(&mut self, packet: &[u8]) -> Result<(), String> {
        self.output.send(packet).map_err(|e| e.to_string())
    }
    fn receive(&mut self, timeout: Duration) -> Result<Vec<u8>, mpsc::RecvTimeoutError> {
        self.replies.recv_timeout(timeout)
    }
    fn drain(&mut self) {
        while self.replies.try_recv().is_ok() {}
    }
}
pub(super) struct MidiStoreWire {
    last_send: Option<Instant>,
    failed: bool,
    pub(super) timeout: Duration,
    io: Box<dyn PacketIo>,
}
impl StoreWire for MidiStoreWire {
    fn query(&mut self, packet: &[u8]) -> Result<Vec<u8>, String> {
        let (operation, request_payload) = unframe(packet)?;
        if self.failed {
            return Err("MIDI read failed; reconnect before reading again".into());
        }
        let editor_read = operation == 1
            && request_payload.len() == 15
            && matches!(request_payload[0..2], [0x1a, 0] | [4, 1] | [0x2e, 0]);
        if !editor_read && !matches!(operation, 0x0c | 0x0e | 0x13) {
            return Err("Unsupported read query".into());
        }
        self.io.drain();
        self.send_once(packet)?;
        let end = Instant::now() + self.timeout;
        while let Some(remaining) = end.checked_duration_since(Instant::now()) {
            let reply = match self.io.receive(remaining) {
                Ok(reply) => reply,
                Err(_) => {
                    // There is no request nonce on this protocol. Never reuse a timed-out
                    // connection: a late matching reply could belong to its previous context.
                    self.failed = true;
                    return Err("MIDI read timed out; reconnect required".into());
                }
            };
            if reply == packet {
                continue;
            }
            if let Ok((op, payload)) = unframe(&reply)
                && op == operation
                && (op != 1 || payload.get(..6) == request_payload.get(..6))
                && (op != 0x0e || payload.first() == request_payload.first())
                && (op != 0x0c || payload != [127])
            {
                return Ok(reply);
            }
        }
        self.failed = true;
        Err("MIDI read timed out; reconnect required".into())
    }
    fn send_once(&mut self, packet: &[u8]) -> Result<(), String> {
        if self.failed {
            return Err("MIDI connection requires reconnect".into());
        }
        if let Some(last) = self.last_send {
            std::thread::sleep(Duration::from_millis(25).saturating_sub(last.elapsed()));
        }
        self.last_send = Some(Instant::now());
        self.io.send(packet).map_err(|e| {
            self.failed = true;
            e.to_string()
        })
    }
    fn settle(&mut self) -> Result<(), String> {
        // Keep the serialized device owner busy for Axe-Edit's 10-second store
        // request window. An echo/other traffic is never treated as an ACK.
        let end = Instant::now() + Duration::from_secs(10);
        while let Some(remaining) = end.checked_duration_since(Instant::now()) {
            match self.io.receive(remaining) {
                Ok(_) => {}
                Err(mpsc::RecvTimeoutError::Timeout) => break,
                Err(mpsc::RecvTimeoutError::Disconnected) => {
                    return Err("MIDI input disconnected".into());
                }
            }
        }
        Ok(())
    }
}

pub fn store_preset(
    input_name: &str,
    output_name: &str,
    expected: &Snapshot,
    destination: u16,
    durable_intent: impl FnOnce() -> Result<(), String>,
) -> Result<(), String> {
    let mut wire = MidiStoreWire::connect(input_name, output_name)?;
    store_preset_with(&mut wire, expected, destination, durable_intent)
}
impl MidiStoreWire {
    pub(super) fn connect(input_name: &str, output_name: &str) -> Result<Self, String> {
        let mut input = MidiInput::new("Axe Studio preset save").map_err(|e| e.to_string())?;
        input.ignore(Ignore::None);
        let output = MidiOutput::new("Axe Studio preset save").map_err(|e| e.to_string())?;
        let inputs = input
            .ports()
            .into_iter()
            .filter(|p| input.port_name(p).ok().as_deref() == Some(input_name))
            .collect::<Vec<_>>();
        let outputs = output
            .ports()
            .into_iter()
            .filter(|p| output.port_name(p).ok().as_deref() == Some(output_name))
            .collect::<Vec<_>>();
        if inputs.len() != 1 || outputs.len() != 1 {
            return Err("Save requires the uniquely selected MIDI input and output".into());
        }
        let (tx, replies) = mpsc::sync_channel::<Vec<u8>>(64);
        let connection = input
            .connect(
                &inputs[0],
                "Axe Studio preset save",
                move |_, bytes, _| {
                    if bytes.len() <= 4096 {
                        let _ = tx.try_send(bytes.to_vec());
                    }
                },
                (),
            )
            .map_err(|e| e.to_string())?;
        let output = output
            .connect(&outputs[0], "Axe Studio preset save")
            .map_err(|e| e.to_string())?;
        Ok(Self {
            io: Box::new(MidiPackets {
                _input: connection,
                output,
                replies,
            }),
            last_send: None,
            failed: false,
            timeout: Duration::from_secs(3),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::protocol::Cell;
    use std::collections::VecDeque;

    fn fixture() -> (Snapshot, Vec<u8>, Vec<u8>) {
        let mut cells = vec![];
        let mut raw = vec![0_u8; 640];
        raw[..4].copy_from_slice(&5_u32.to_le_bytes());
        raw[16..20].copy_from_slice(b"Test");
        for column in 0..14 {
            for row in 0..6 {
                let (instance, inputs): (u16, u16) = match (column, row) {
                    (0, 0) => (58, 0),
                    (1, 0) => (62, 1),
                    _ => (0, 0),
                };
                let offset = 304 + (column * 6 + row) * 4;
                raw[offset..offset + 2].copy_from_slice(&instance.to_le_bytes());
                raw[offset + 2..offset + 4].copy_from_slice(&inputs.to_le_bytes());
                cells.push(Cell {
                    row: row as u8 + 1,
                    column: column as u8 + 1,
                    instance,
                    inputs,
                });
            }
        }
        // Authored bitstream packs raw bytes independently of production unpack.
        let bits: Vec<_> = raw
            .iter()
            .flat_map(|b| (0..8).rev().map(move |bit| (b >> bit) & 1))
            .collect();
        let packed: Vec<u8> = bits
            .chunks(7)
            .map(|chunk| {
                chunk
                    .iter()
                    .enumerate()
                    .fold(0, |value, (i, b)| value | (b << (6 - i)))
            })
            .collect();
        let mut payload = vec![0; 15];
        payload[0] = 0x2e;
        payload[13] = 0;
        payload[14] = 5;
        payload.extend(packed);
        let mut snapshot = super::super::snapshot(
            Grid {
                slot: 5,
                name: "Test".into(),
                cells,
            },
            2,
        )
        .unwrap();
        for b in &mut snapshot.blocks {
            b.channel = Some(0);
            b.bypassed = Some(false);
        }
        (
            snapshot,
            frame(1, &payload).unwrap(),
            frame(0x13, &[58, 0, 64, 62, 0, 64]).unwrap(),
        )
    }
    struct Wire {
        replies: VecDeque<Result<Vec<u8>, String>>,
        sends: Vec<Vec<u8>>,
        send_fails: bool,
        settle_fails: bool,
        settles: usize,
    }
    impl Wire {
        fn new(grid: &[u8], status: &[u8]) -> Self {
            Self {
                replies: [
                    grid.to_vec(),
                    frame(0x0c, &[2]).unwrap(),
                    status.to_vec(),
                    grid.to_vec(),
                    frame(0x0c, &[2]).unwrap(),
                    status.to_vec(),
                ]
                .into_iter()
                .map(Ok)
                .collect(),
                sends: vec![],
                send_fails: false,
                settle_fails: false,
                settles: 0,
            }
        }
    }
    impl StoreWire for Wire {
        fn query(&mut self, _: &[u8]) -> Result<Vec<u8>, String> {
            self.replies.pop_front().unwrap_or(Err("timeout".into()))
        }
        fn send_once(&mut self, bytes: &[u8]) -> Result<(), String> {
            self.sends.push(bytes.to_vec());
            if self.send_fails {
                Err("disconnected".into())
            } else {
                Ok(())
            }
        }
        fn settle(&mut self) -> Result<(), String> {
            self.settles += 1;
            if self.settle_fails {
                Err("disconnected".into())
            } else {
                Ok(())
            }
        }
    }
    #[test]
    fn hardware_store_sends_exact_integer_destination_once_after_durable_intent() {
        let (snapshot, grid, status) = fixture();
        let mut wire = Wire::new(&grid, &status);
        let mut intents = 0;
        store_preset_with(&mut wire, &snapshot, 1023, || {
            intents += 1;
            Ok(())
        })
        .unwrap();
        assert_eq!(intents, 1);
        assert_eq!(wire.settles, 1);
        assert_eq!(wire.sends.len(), 1);
        let packet = &wire.sends[0];
        assert_eq!(&packet[..12], &[240, 0, 1, 116, 16, 1, 38, 0, 0, 0, 0, 0]);
        assert_eq!(&packet[12..21], &[127, 7, 0, 0, 0, 0, 0, 0, 0]);
        assert_eq!(packet.len(), 23);
        assert_eq!(packet[21], packet[..21].iter().fold(0, |v, b| v ^ b) & 127);
        assert_eq!(packet[22], 247);
    }
    #[test]
    fn stale_context_invalid_destination_and_failed_journal_send_nothing() {
        let (snapshot, grid, status) = fixture();
        for case in 0..12 {
            let mut snapshot = snapshot.clone();
            let mut wire = Wire::new(&grid, &status);
            let destination = if case == 0 { 1024 } else { 17 };
            match case {
                1 => snapshot.context.preset = 6,
                2 => snapshot.name = "Other".into(),
                3 => snapshot.context.scene = Some(3),
                4 => snapshot.blocks[0].channel = Some(1),
                5 => {
                    wire.replies[4] = Ok(frame(0x0c, &[3]).unwrap());
                }
                6 => {
                    wire.replies[0] = Err("timeout".into());
                }
                8 => snapshot.edges.clear(),
                9 => snapshot.context.target = Target::TestMock,
                10 => wire.replies[2] = Ok(frame(0x13, &[58, 0, 126]).unwrap()),
                11 => wire.replies[2] = Ok(frame(0x13, &[58, 0, 64, 58, 0, 64]).unwrap()),
                _ => {}
            }
            let result = store_preset_with(&mut wire, &snapshot, destination, || {
                if case == 7 {
                    Err("journal unavailable".into())
                } else {
                    Ok(())
                }
            });
            assert!(result.is_err(), "case {case}");
            assert!(wire.sends.is_empty(), "case {case}");
            assert_eq!(wire.settles, 0);
        }
    }
    #[test]
    fn disconnect_is_uncertain_and_never_retried() {
        let (snapshot, grid, status) = fixture();
        for during_settle in [false, true] {
            let mut wire = Wire::new(&grid, &status);
            wire.send_fails = !during_settle;
            wire.settle_fails = during_settle;
            let error = store_preset_with(&mut wire, &snapshot, 17, || Ok(())).unwrap_err();
            assert!(error.contains(if during_settle {
                "request was sent"
            } else {
                "delivery is uncertain"
            }));
            assert_eq!(wire.sends.len(), 1);
        }
    }
    type SentPackets = std::sync::Arc<std::sync::Mutex<Vec<(Instant, Vec<u8>)>>>;
    struct Packets {
        replies: VecDeque<Result<Vec<u8>, mpsc::RecvTimeoutError>>,
        sends: SentPackets,
    }
    impl PacketIo for Packets {
        fn send(&mut self, packet: &[u8]) -> Result<(), String> {
            self.sends
                .lock()
                .unwrap()
                .push((Instant::now(), packet.to_vec()));
            Ok(())
        }
        fn receive(&mut self, _: Duration) -> Result<Vec<u8>, mpsc::RecvTimeoutError> {
            self.replies
                .pop_front()
                .unwrap_or(Err(mpsc::RecvTimeoutError::Timeout))
        }
        fn drain(&mut self) {}
    }
    fn injected(
        replies: Vec<Result<Vec<u8>, mpsc::RecvTimeoutError>>,
    ) -> (MidiStoreWire, SentPackets) {
        let sends = std::sync::Arc::new(std::sync::Mutex::new(vec![]));
        (
            MidiStoreWire {
                last_send: None,
                failed: false,
                timeout: Duration::from_millis(1),
                io: Box::new(Packets {
                    replies: replies.into(),
                    sends: sends.clone(),
                }),
            },
            sends,
        )
    }
    fn token_reply(effect: u16, value: u8) -> Vec<u8> {
        let request = crate::protocol::block_token_getter(effect).unwrap();
        let (_, p) = unframe(&request).unwrap();
        let mut p = p.to_vec();
        p[6] = value;
        frame(1, &p).unwrap()
    }
    #[test]
    fn persistent_wire_serializes_scene_save_and_polling_with_rate_limit() {
        let (snapshot, grid, status) = fixture();
        let replies = vec![
            Ok(frame(0x0c, &[2]).unwrap()),
            Ok(token_reply(58, 1)),
            Ok(grid.clone()),
            Ok(frame(0x0c, &[2]).unwrap()),
            Ok(status.clone()),
            Ok(grid),
            Ok(frame(0x0c, &[2]).unwrap()),
            Ok(status),
            Err(mpsc::RecvTimeoutError::Timeout),
            Ok(token_reply(58, 2)),
        ];
        let (mut wire, sends) = injected(replies);
        super::super::select_scene_with(&mut wire, 2).unwrap();
        let query = crate::protocol::block_token_getter(58).unwrap();
        wire.query(&query).unwrap();
        let mut intents = 0;
        store_preset_with(&mut wire, &snapshot, 17, || {
            intents += 1;
            Ok(())
        })
        .unwrap();
        wire.query(&query).unwrap();
        assert_eq!(intents, 1);
        let sends = sends.lock().unwrap();
        let store_index = sends
            .iter()
            .position(|(_, p)| p == &preset_store(17).unwrap())
            .unwrap();
        assert_eq!(store_index, 9);
        assert_eq!(sends.len(), 11);
        assert_eq!(
            sends
                .iter()
                .filter(|(_, p)| p == &preset_store(17).unwrap())
                .count(),
            1
        );
        for pair in sends.windows(2) {
            assert!(pair[1].0.duration_since(pair[0].0) >= Duration::from_millis(25));
        }
    }
    #[test]
    fn timed_out_wire_rejects_late_reply_and_requires_a_new_connection() {
        let query = crate::protocol::block_token_getter(58).unwrap();
        let (mut old, sends) = injected(vec![
            Err(mpsc::RecvTimeoutError::Timeout),
            Ok(token_reply(58, 1)),
        ]);
        assert!(old.query(&query).is_err());
        assert!(old.query(&query).is_err());
        assert!(old.send_once(&preset_store(17).unwrap()).is_err());
        assert_eq!(sends.lock().unwrap().len(), 1);
        let (mut new, _) = injected(vec![Ok(token_reply(58, 2))]);
        assert_eq!(
            crate::protocol::block_token_readback(&new.query(&query).unwrap(), 58)
                .unwrap()
                .0,
            2
        );
    }
    #[test]
    fn persistent_wire_ignores_other_block_replies_and_echoes() {
        let query = crate::protocol::block_token_getter(58).unwrap();
        let (mut wire, _) = injected(vec![
            Ok(query.clone()),
            Ok(token_reply(59, 2)),
            Ok(token_reply(58, 3)),
        ]);
        assert_eq!(
            crate::protocol::block_token_readback(&wire.query(&query).unwrap(), 58)
                .unwrap()
                .0,
            3
        );
    }
}
