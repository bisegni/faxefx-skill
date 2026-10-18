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

struct MidiStoreWire {
    _input: midir::MidiInputConnection<()>,
    output: midir::MidiOutputConnection,
    replies: mpsc::Receiver<Vec<u8>>,
}
impl StoreWire for MidiStoreWire {
    fn query(&mut self, packet: &[u8]) -> Result<Vec<u8>, String> {
        let (operation, request_payload) = unframe(packet)?;
        // The read boundary accepts only the three exact preflight queries.
        if packet != grid_query() && packet != frame(0x0c, &[127])? && packet != frame(0x13, &[])? {
            return Err("Unsupported save preflight query".into());
        }
        while self.replies.try_recv().is_ok() {}
        self.output.send(packet).map_err(|e| e.to_string())?;
        let end = Instant::now() + Duration::from_secs(3);
        while let Some(remaining) = end.checked_duration_since(Instant::now()) {
            let reply = self
                .replies
                .recv_timeout(remaining)
                .map_err(|_| "Save preflight read timed out; no store was sent")?;
            if reply == packet {
                continue;
            }
            if let Ok((op, payload)) = unframe(&reply)
                && op == operation
                && (op != 1 || payload.get(..6) == request_payload.get(..6))
            {
                return Ok(reply);
            }
        }
        Err("Save preflight timed out; no store was sent".into())
    }
    fn send_once(&mut self, packet: &[u8]) -> Result<(), String> {
        self.output.send(packet).map_err(|e| e.to_string())
    }
    fn settle(&mut self) -> Result<(), String> {
        // Keep the serialized device owner busy for Axe-Edit's 10-second store
        // request window. An echo/other traffic is never treated as an ACK.
        let end = Instant::now() + Duration::from_secs(10);
        while let Some(remaining) = end.checked_duration_since(Instant::now()) {
            match self.replies.recv_timeout(remaining) {
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
    preset_store(destination)?;
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
    store_preset_with(
        &mut MidiStoreWire {
            _input: connection,
            output,
            replies,
        },
        expected,
        destination,
        durable_intent,
    )
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
}
