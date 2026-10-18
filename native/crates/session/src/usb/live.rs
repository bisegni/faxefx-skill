//! Incremental selected-block synchronization. One step performs at most one read.
//! The caller owns scheduling and serializes explicit commands on the same wire.
use super::*;
use crate::protocol::{
    BlockToken, amp_readback, block_token_getter, block_token_readback, parameter_getter,
    parameter_readback,
};
use std::collections::VecDeque;

pub fn unavailable_parameter(id: u16, label: String) -> Parameter {
    Parameter {
        id: id.to_string(),
        label,
        unit: "device text".into(),
        confirmed: None,
        display_text: None,
        raw_numeric: None,
        read_status: Some("unavailable".into()),
        read_error: None,
        range: None,
        writable: false,
        automated: None,
        raw_bits: None,
    }
}
fn decode(bytes: &[u8], effect: u16, source: &Parameter) -> Result<Parameter, String> {
    let id = source.id.parse().map_err(|_| "Invalid parameter ID")?;
    let (bits, raw, text) = parameter_readback(bytes, effect, id)?;
    let mut p = unavailable_parameter(id, source.label.clone());
    p.raw_bits = Some(bits);
    p.raw_numeric = Some(raw);
    p.display_text = (!text.is_empty()).then_some(text);
    let verified = (58..=61).contains(&effect) && [0, 1, 2, 3, 4, 5, 20, 21].contains(&id);
    if verified {
        match amp_readback(bytes, effect, id) {
            Ok((_, value, _)) => {
                p.confirmed = value;
                p.range = value.map(|_| (0., 10.));
            }
            Err(error) => {
                p.read_error = Some(error);
            }
        }
    }
    p.unit = if p.confirmed.is_some() {
        "knob"
    } else {
        "device text"
    }
    .into();
    p.read_status = Some(
        if p.read_error.is_some() {
            "conversion-unverified"
        } else if verified {
            "verified-device-read"
        } else {
            "device-reply-unverified-getter"
        }
        .into(),
    );
    Ok(p)
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct Freshness {
    pub connection_generation: u64,
    pub context_generation: u64,
    pub observed_ms: u64,
    pub token: Option<u32>,
    pub fresh: bool,
}
struct Refresh {
    token: Option<BlockToken>,
    pending: VecDeque<String>,
}
pub struct Synchronizer {
    pub snapshot: Snapshot,
    pub freshness: BTreeMap<String, Freshness>,
    selected: Option<String>,
    visible: Vec<String>,
    latest: Option<BlockToken>,
    acknowledged: Option<BlockToken>,
    refresh: Option<Refresh>,
    token_due: u64,
    context_due: u64,
    context_phase: u8,
    scene: Option<u8>,
    retry_at: u64,
    failures: u32,
    generation: u64,
    next_read: u64,
}
impl Synchronizer {
    pub fn new(snapshot: Snapshot) -> Self {
        Self {
            generation: snapshot.context.generation,
            next_read: 0,
            snapshot,
            freshness: BTreeMap::new(),
            selected: None,
            visible: vec![],
            latest: None,
            acknowledged: None,
            refresh: None,
            token_due: 0,
            context_due: 0,
            context_phase: 0,
            scene: None,
            retry_at: 0,
            failures: 0,
        }
    }
    pub fn select(&mut self, selected: Option<String>, visible: Vec<String>) {
        let visible: Vec<_> = visible.into_iter().take(128).collect();
        if selected != self.selected {
            for f in self.freshness.values_mut() {
                f.fresh = false;
            }
            for b in &mut self.snapshot.blocks {
                for p in &mut b.parameters {
                    if p.raw_bits.is_some() {
                        p.read_status = Some("cached · selection changed".into());
                    }
                }
            }
            self.selected = selected;
            self.visible = visible;
            self.latest = None;
            self.acknowledged = None;
            self.refresh = None;
            self.token_due = 0;
            self.context_due = 0;
            self.context_phase = 0;
            self.begin_refresh();
        } else if self.visible != visible {
            let newly_visible: Vec<_> = visible
                .iter()
                .filter(|id| !self.visible.contains(id))
                .cloned()
                .collect();
            self.visible = visible;
            self.begin_refresh();
            if let Some(refresh) = &mut self.refresh {
                // Model stays first; newly exposed rows precede already cached rows.
                let amp = self
                    .snapshot
                    .blocks
                    .iter()
                    .find(|b| Some(&b.id) == self.selected.as_ref())
                    .and_then(|b| b.instance_id)
                    .is_some_and(|id| (58..=61).contains(&id));
                refresh.pending.make_contiguous().sort_by_key(|id| {
                    if amp && id == "0" {
                        0
                    } else if newly_visible.contains(id) {
                        1
                    } else {
                        2
                    }
                });
            }
        }
    }
    fn block(&self) -> Option<&Block> {
        self.snapshot
            .blocks
            .iter()
            .find(|b| Some(&b.id) == self.selected.as_ref())
    }
    fn begin_refresh(&mut self) {
        let mut ids = self.visible.clone();
        if self
            .block()
            .and_then(|b| b.instance_id)
            .is_some_and(|id| (58..=61).contains(&id))
        {
            ids.retain(|id| id != "0");
            ids.insert(0, "0".into());
        }
        self.refresh = Some(Refresh {
            token: self.latest,
            pending: ids.into(),
        });
    }
    fn invalidate(&mut self) {
        self.snapshot.context.generation += 1;
        for b in &mut self.snapshot.blocks {
            for p in &mut b.parameters {
                *p = unavailable_parameter(p.id.parse().unwrap_or(0), p.label.clone());
            }
        }
        for f in self.freshness.values_mut() {
            f.fresh = false;
        }
        self.latest = None;
        self.acknowledged = None;
        self.begin_refresh();
        self.token_due = 0;
    }
    /// No operation is replayed here. Failure retries apply only to reads.
    pub fn step(&mut self, wire: &mut impl StoreWire, now: u64) -> Result<(), String> {
        if now < self.retry_at || now < self.next_read {
            return Ok(());
        }
        self.next_read = now + 25;
        let result = self.read_step(wire, now);
        if let Err(error) = &result {
            self.failures = (self.failures + 1).min(6);
            self.retry_at = now + (250_u64 << self.failures);
            self.invalidate();
            for b in &mut self.snapshot.blocks {
                for p in &mut b.parameters {
                    p.read_error = Some(error.clone());
                }
            }
        } else {
            self.failures = 0;
        }
        result
    }
    fn read_step(&mut self, wire: &mut impl StoreWire, now: u64) -> Result<(), String> {
        // Check preset identity as well: a preset switch can preserve scene/channel IDs.
        if self.context_phase == 2 {
            let grid = decode_grid(&wire.query(&grid_query())?)?;
            let current = super::snapshot(grid.clone(), self.snapshot.context.scene.unwrap_or(0))?;
            let placed = |s: &Snapshot| {
                s.blocks
                    .iter()
                    .map(|b| (b.id.clone(), b.instance_id, b.position))
                    .collect::<Vec<_>>()
            };
            if grid.slot != self.snapshot.context.preset
                || grid.name != self.snapshot.name
                || placed(&current) != placed(&self.snapshot)
                || current.edges != self.snapshot.edges
            {
                let context = self.snapshot.context.clone();
                self.snapshot = current;
                self.snapshot.context.session = context.session;
                self.snapshot.context.generation = context.generation;
                for block in &mut self.snapshot.blocks {
                    if let Some(prefix) = &block.parameter_prefix {
                        block.parameters = parameter_specs(prefix)
                            .into_iter()
                            .map(|(id, label)| unavailable_parameter(id, label))
                            .collect();
                    }
                }
                self.invalidate();
            }
            self.context_phase = 0;
            return Ok(());
        }
        let finishing = self.refresh.as_ref().is_some_and(|r| r.pending.is_empty());
        if self.context_phase == 1 {
            let packet = wire.query(&frame(0x13, &[])?)?;
            let (op, payload) = unframe(&packet)?;
            if op != 0x13 || !payload.len().is_multiple_of(3) {
                return Err("Invalid live block status".into());
            }
            let mut status = BTreeMap::new();
            for r in payload.as_chunks::<3>().0 {
                let id = r[0] as u16 | ((r[1] as u16) << 7);
                let channel = r[2] >> 1 & 7;
                let count = r[2] >> 4 & 7;
                if count > 4
                    || channel >= count.max(1)
                    || status.insert(id, (channel, r[2] & 1 != 0)).is_some()
                {
                    return Err("Invalid live channel status".into());
                }
            }
            let mut changed = self.snapshot.context.scene != self.scene;
            for b in &mut self.snapshot.blocks {
                if let Some(id) = b.instance_id {
                    if id >= 1024 {
                        continue;
                    }
                    let Some((channel, bypassed)) = status.get(&id).copied() else {
                        changed = true;
                        b.channel = None;
                        b.bypassed = None;
                        continue;
                    };
                    changed |= self.selected.as_ref() == Some(&b.id) && b.channel != Some(channel);
                    b.channel = Some(channel);
                    b.bypassed = Some(bypassed);
                }
            }
            self.snapshot.context.scene = self.scene;
            let amps: Vec<_> = self
                .snapshot
                .blocks
                .iter()
                .filter(|b| b.instance_id.is_some_and(|id| (58..=61).contains(&id)))
                .collect();
            self.snapshot.context.channel = if amps.len() == 1 {
                amps[0].channel
            } else {
                None
            };
            self.context_phase = 2;
            self.context_due = now + 475;
            if changed {
                self.invalidate();
            } else if finishing {
                self.publish(now);
            }
            return Ok(());
        }
        if now >= self.context_due || finishing {
            self.scene = Some(read_scene(&wire.query(&frame(0x0c, &[127])?)?)?);
            self.context_phase = 1;
            return Ok(());
        }
        let Some(block) = self.block() else {
            return Ok(());
        };
        let Some(effect) = block.instance_id else {
            return Ok(());
        };
        if now >= self.token_due {
            let token = block_token_readback(&wire.query(&block_token_getter(effect)?)?, effect)?;
            self.token_due = now + 100;
            if self.latest != Some(token) {
                self.latest = Some(token);
                for (key, f) in &mut self.freshness {
                    if key.starts_with(&format!(
                        "{}/",
                        self.selected.as_deref().unwrap_or_default()
                    )) {
                        f.fresh = false;
                    }
                }
                if let Some(block) = self
                    .snapshot
                    .blocks
                    .iter_mut()
                    .find(|b| Some(&b.id) == self.selected.as_ref())
                {
                    for p in &mut block.parameters {
                        if p.raw_bits.is_some() {
                            p.read_status = Some("cached · refresh pending".into());
                        }
                    }
                }
                if self.refresh.is_none() {
                    self.begin_refresh();
                } else if let Some(r) = &mut self.refresh {
                    // The initial batch starts before the first token reply.
                    if r.token.is_none() {
                        r.token = Some(token);
                    }
                }
            }
            return Ok(());
        }
        let id = self
            .refresh
            .as_ref()
            .and_then(|r| r.pending.front())
            .cloned();
        if let Some(id) = id {
            let source = self
                .block()
                .and_then(|b| b.parameters.iter().find(|p| p.id == id))
                .cloned();
            let value = if let Some(source) = source {
                let parameter = id.parse().map_err(|_| "Invalid catalog parameter")?;
                Some(decode(
                    &wire.query(&parameter_getter(effect, parameter)?)?,
                    effect,
                    &source,
                )?)
            } else {
                None
            };
            let token = self.refresh.as_ref().and_then(|r| r.token);
            if let Some(value) = value {
                self.apply_value(value, token, now);
            }
            self.refresh.as_mut().unwrap().pending.pop_front();
        }
        Ok(())
    }
    fn apply_value(&mut self, mut value: Parameter, token: Option<BlockToken>, now: u64) {
        if token != self.latest {
            value.read_status = Some(format!(
                "cached · {}",
                value.read_status.as_deref().unwrap_or("unavailable")
            ));
        }
        let Some(selected) = self.selected.clone() else {
            return;
        };
        let Some(block) = self.snapshot.blocks.iter_mut().find(|b| b.id == selected) else {
            return;
        };
        if value.id == "0" && block.instance_id.is_some_and(|id| (58..=61).contains(&id)) {
            let old = block.parameters.iter().find(|p| p.id == "0");
            if old.is_some_and(|p| p.raw_bits.is_some() && p.raw_bits != value.raw_bits) {
                for p in &mut block.parameters {
                    *p = unavailable_parameter(p.id.parse().unwrap_or(0), p.label.clone());
                }
                for f in self.freshness.values_mut() {
                    f.fresh = false;
                }
            }
        }
        let key = format!("{selected}/{}", value.id);
        self.freshness.insert(
            key,
            Freshness {
                connection_generation: self.generation,
                context_generation: self.snapshot.context.generation,
                observed_ms: now,
                token: token.map(|t| t.0),
                fresh: token == self.latest,
            },
        );
        if let Some(p) = block.parameters.iter_mut().find(|p| p.id == value.id) {
            *p = value;
        }
    }
    fn publish(&mut self, _now: u64) {
        let Some(refresh) = self.refresh.take() else {
            return;
        };
        self.acknowledged = refresh.token;
        if self.latest != self.acknowledged {
            self.begin_refresh();
        }
    }
}

/// Backend-owned persistent MIDI connection. Capture, live reads, scene and Save
/// all use this same owner. It is never reconstructed by a background refresh.
pub struct MidiService {
    wire: store::MidiStoreWire,
    pub live: Synchronizer,
    started: Instant,
}
impl MidiService {
    pub fn connect(input: &str, output: &str) -> Result<Self, String> {
        let mut wire = store::MidiStoreWire::connect(input, output)?;
        let mut snapshot = capture_with(&mut wire)?;
        static GENERATION: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);
        snapshot.context.generation = GENERATION.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        Ok(Self {
            wire,
            live: Synchronizer::new(snapshot),
            started: Instant::now(),
        })
    }
    pub fn step(&mut self, selected: Option<String>, visible: Vec<String>) -> Result<(), String> {
        self.live.select(selected, visible);
        self.wire.timeout = Duration::from_millis(250);
        let result = self
            .live
            .step(&mut self.wire, self.started.elapsed().as_millis() as u64);
        self.wire.timeout = Duration::from_secs(3);
        result
    }
    pub fn scene(&mut self, scene: u8) -> Result<Snapshot, String> {
        select_scene_with(&mut self.wire, scene)?;
        let mut snapshot = capture_with(&mut self.wire)?;
        snapshot.context.session = self.live.snapshot.context.session.clone();
        snapshot.context.generation = self.live.snapshot.context.generation + 1;
        let connection_generation = self.live.generation;
        self.live = Synchronizer::new(snapshot);
        self.live.generation = connection_generation;
        Ok(self.live.snapshot.clone())
    }
    pub fn save(
        &mut self,
        expected: &Snapshot,
        destination: u16,
        before: &mut dyn FnMut() -> Result<(), String>,
    ) -> Result<(), String> {
        store_preset_with(&mut self.wire, expected, destination, before)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::protocol::Cell;
    #[derive(Default)]
    struct Wire {
        token: u32,
        scene: u8,
        channel: u8,
        other_channel: u8,
        model: u32,
        preset: u16,
        mismatch: bool,
        fail: bool,
        unknown_conversion: bool,
        reads: Vec<(u16, u16, u16)>,
        writes: usize,
    }
    fn editor_reply(request: &[u8], bits: u32, text: &str) -> Vec<u8> {
        let (_, payload) = unframe(request).unwrap();
        let mut p = payload[..15].to_vec();
        for i in 0..5 {
            p[6 + i] = ((bits as u64 >> (7 * i)) & 127) as u8;
        }
        p[13] = (text.len() & 127) as u8;
        p[14] = (text.len() >> 7) as u8;
        let bits: Vec<_> = text
            .bytes()
            .flat_map(|b| (0..8).rev().map(move |i| (b >> i) & 1))
            .collect();
        p.extend(bits.chunks(7).map(|bits| {
            bits.iter()
                .enumerate()
                .fold(0, |a, (i, b)| a | (b << (6 - i)))
        }));
        frame(1, &p).unwrap()
    }
    impl StoreWire for Wire {
        fn query(&mut self, request: &[u8]) -> Result<Vec<u8>, String> {
            let (op, p) = unframe(request)?;
            if self.fail {
                return Err("disconnected".into());
            }
            match op {
                0x0c => {
                    self.reads.push((0x0c, 0, 0));
                    frame(op, &[self.scene])
                }
                0x13 => {
                    self.reads.push((0x13, 0, 0));
                    frame(
                        op,
                        &[
                            58,
                            0,
                            64 | self.channel << 1,
                            59,
                            0,
                            64 | self.other_channel << 1,
                        ],
                    )
                }
                1 => {
                    let operation = p[0] as u16 | (p[1] as u16) << 7;
                    let effect = p[2] as u16 | (p[3] as u16) << 7;
                    let parameter = p[4] as u16 | (p[5] as u16) << 7;
                    self.reads.push((operation, effect, parameter));
                    let reply = if operation == 0x2e {
                        let mut raw = vec![0u8; 640];
                        raw[..4].copy_from_slice(&(5_u32 + self.preset as u32).to_le_bytes());
                        raw[16..24].copy_from_slice(b"Authored");
                        raw[304..306].copy_from_slice(&58_u16.to_le_bytes());
                        raw[308..310].copy_from_slice(&59_u16.to_le_bytes());
                        let bits: Vec<_> = raw
                            .iter()
                            .flat_map(|b| (0..8).rev().map(move |i| (b >> i) & 1))
                            .collect();
                        let mut payload = p.to_vec();
                        payload[13] = 0;
                        payload[14] = 5;
                        payload.extend(bits.chunks(7).map(|bits| {
                            bits.iter()
                                .enumerate()
                                .fold(0, |a, (i, b)| a | (b << (6 - i)))
                        }));
                        frame(1, &payload)?
                    } else if operation == 0x84 {
                        editor_reply(request, self.token, "")
                    } else if parameter == 0 {
                        editor_reply(request, self.model, "USA Clean")
                    } else {
                        editor_reply(
                            request,
                            0.5_f32.to_bits(),
                            if self.unknown_conversion {
                                "500 Hz"
                            } else {
                                "5.00"
                            },
                        )
                    };
                    if self.mismatch {
                        let (_, payload) = unframe(&reply)?;
                        let mut payload = payload.to_vec();
                        payload[2] = 60;
                        frame(1, &payload)
                    } else {
                        Ok(reply)
                    }
                }
                _ => Err("unexpected query".into()),
            }
        }
        fn send_once(&mut self, _: &[u8]) -> Result<(), String> {
            self.writes += 1;
            Ok(())
        }
        fn settle(&mut self) -> Result<(), String> {
            Ok(())
        }
    }
    fn synchronizer() -> Synchronizer {
        let grid = Grid {
            slot: 5,
            name: "Authored".into(),
            cells: vec![
                Cell {
                    row: 1,
                    column: 1,
                    instance: 58,
                    inputs: 0,
                },
                Cell {
                    row: 2,
                    column: 1,
                    instance: 59,
                    inputs: 0,
                },
            ],
        };
        let mut s = super::super::snapshot(grid, 0).unwrap();
        for b in &mut s.blocks {
            b.channel = Some(0);
            b.bypassed = Some(false);
            b.parameters = (0..22)
                .map(|id| unavailable_parameter(id, format!("P{id}")))
                .collect();
        }
        let mut sync = Synchronizer::new(s);
        sync.select(Some("cell-r1c1".into()), vec!["1".into(), "2".into()]);
        sync
    }
    fn run(sync: &mut Synchronizer, wire: &mut Wire, from: u64, until: u64) {
        for now in (from..until).step_by(25) {
            sync.step(wire, now).unwrap();
        }
    }
    fn parameter_reads(wire: &Wire) -> Vec<(u16, u16, u16)> {
        wire.reads.iter().copied().filter(|r| r.0 == 0x1a).collect()
    }
    #[test]
    fn unchanged_tokens_do_not_read_parameters_or_unrelated_blocks() {
        let mut sync = synchronizer();
        let mut wire = Wire::default();
        run(&mut sync, &mut wire, 0, 1000);
        assert_eq!(
            parameter_reads(&wire),
            vec![(0x1a, 58, 0), (0x1a, 58, 1), (0x1a, 58, 2)]
        );
        wire.reads.clear();
        wire.other_channel = 2;
        run(&mut sync, &mut wire, 1000, 2000);
        assert!(parameter_reads(&wire).is_empty());
        assert_eq!(wire.writes, 0);
    }
    #[test]
    fn changed_token_and_scroll_read_only_selected_visible_rows() {
        let mut sync = synchronizer();
        let mut wire = Wire::default();
        run(&mut sync, &mut wire, 0, 1000);
        wire.token = 1;
        wire.reads.clear();
        run(&mut sync, &mut wire, 1000, 2000);
        assert_eq!(parameter_reads(&wire).len(), 3);
        assert_eq!(sync.block().unwrap().parameters[1].confirmed, Some(5.));
        sync.select(Some("cell-r1c1".into()), vec!["3".into()]);
        wire.reads.clear();
        run(&mut sync, &mut wire, 2000, 3000);
        assert_eq!(parameter_reads(&wire), vec![(0x1a, 58, 0), (0x1a, 58, 3)]);
        assert_eq!(sync.block().unwrap().parameters[1].confirmed, Some(5.));
        assert!(sync.freshness.contains_key("cell-r1c1/1"));
        sync.select(Some("cell-r2c1".into()), vec!["1".into()]);
        wire.reads.clear();
        run(&mut sync, &mut wire, 3000, 4000);
        assert!(
            wire.reads
                .iter()
                .filter(|r| r.0 == 0x84 || r.0 == 0x1a)
                .all(|r| r.1 == 59)
        );
    }
    #[test]
    fn changes_during_refresh_are_not_acknowledged_early() {
        let mut sync = synchronizer();
        let mut wire = Wire::default();
        // Token 0 starts the batch. Token 1 arrives before its final row completes.
        run(&mut sync, &mut wire, 0, 125);
        wire.token = 1;
        run(&mut sync, &mut wire, 125, 1000);
        assert_eq!(sync.acknowledged, Some(BlockToken(1)));
        assert!(parameter_reads(&wire).iter().filter(|r| r.2 == 1).count() >= 2);
        assert!(sync.refresh.is_none());
    }
    #[test]
    fn scene_channel_and_model_changes_invalidate_values() {
        let mut sync = synchronizer();
        let mut wire = Wire::default();
        run(&mut sync, &mut wire, 0, 1000);
        wire.scene = 2;
        for now in (1000..2000).step_by(25) {
            sync.step(&mut wire, now).unwrap();
            if sync.snapshot.context.scene == Some(2) {
                break;
            }
        }
        assert!(
            sync.block()
                .unwrap()
                .parameters
                .iter()
                .all(|p| p.confirmed.is_none())
        );
        run(&mut sync, &mut wire, 2000, 3000);
        wire.channel = 1;
        for now in (3000..4000).step_by(25) {
            sync.step(&mut wire, now).unwrap();
            if sync.block().unwrap().channel == Some(1) {
                break;
            }
        }
        assert!(
            sync.block()
                .unwrap()
                .parameters
                .iter()
                .all(|p| p.confirmed.is_none())
        );
        run(&mut sync, &mut wire, 4000, 5000);
        wire.model = 1;
        wire.token += 1;
        // A changed model must discard old knob readings before subsequent numeric reads.
        for now in (5000..6000).step_by(25) {
            sync.step(&mut wire, now).unwrap();
            if sync.block().unwrap().parameters[0].raw_bits == Some(1) {
                break;
            }
        }
        assert!(sync.block().unwrap().parameters[1].confirmed.is_none());
    }
    #[test]
    fn device_text_and_unknown_conversions_never_confirm_knobs() {
        let mut sync = synchronizer();
        let mut wire = Wire {
            unknown_conversion: true,
            ..Wire::default()
        };
        run(&mut sync, &mut wire, 0, 1000);
        let p = &sync.block().unwrap().parameters[1];
        assert_eq!(p.display_text.as_deref(), Some("500 Hz"));
        assert_eq!(p.confirmed, None);
        assert_eq!(p.range, None);
        assert!(p.read_error.is_some());
        let (_, raw, text) = parameter_readback(
            &editor_reply(&parameter_getter(58, 1).unwrap(), 0.5_f32.to_bits(), "5.00"),
            58,
            1,
        )
        .unwrap();
        assert_eq!(raw, 0.5);
        assert_eq!(text, "5.00");
    }
    #[test]
    fn mismatched_late_reply_disconnect_backoff_and_reconnect() {
        let mut sync = synchronizer();
        let mut wire = Wire::default();
        run(&mut sync, &mut wire, 0, 1000);
        wire.mismatch = true;
        sync.token_due = 0;
        sync.context_phase = 0;
        sync.context_due = u64::MAX;
        assert!(sync.step(&mut wire, 1000).is_err());
        assert!(
            sync.block()
                .unwrap()
                .parameters
                .iter()
                .all(|p| p.confirmed.is_none())
        );
        let reads = wire.reads.len();
        sync.step(&mut wire, 1025).unwrap();
        assert_eq!(wire.reads.len(), reads);
        wire.fail = true;
        assert!(sync.step(&mut wire, 2000).is_err());
        let old_session = sync.snapshot.context.session.clone();
        let mut reconnected = synchronizer();
        reconnected.snapshot.context.session = "new connection".into();
        reconnected.snapshot.context.generation = 99;
        let mut wire = Wire::default();
        run(&mut reconnected, &mut wire, 0, 1000);
        assert_ne!(reconnected.snapshot.context.session, old_session);
        assert_eq!(
            reconnected.block().unwrap().parameters[1].confirmed,
            Some(5.)
        );
    }
    #[test]
    fn preset_changes_with_identical_scene_and_channels_invalidate_context() {
        let mut sync = synchronizer();
        let mut wire = Wire::default();
        run(&mut sync, &mut wire, 0, 1000);
        wire.preset = 1;
        run(&mut sync, &mut wire, 1000, 2000);
        assert_eq!(sync.snapshot.context.preset, 6);
        assert!(sync.snapshot.context.generation > 1);
    }
    #[test]
    fn rapid_ticks_have_bounded_traffic_and_single_read_steps() {
        let mut sync = synchronizer();
        let mut wire = Wire::default();
        for now in 0..1000 {
            let count = wire.reads.len();
            sync.step(&mut wire, now).unwrap();
            assert!(wire.reads.len() <= count + 1);
        }
        assert!(wire.reads.len() <= 40);
        sync.select(
            Some("cell-r1c1".into()),
            (0..1000).map(|n| n.to_string()).collect(),
        );
        assert!(sync.visible.len() <= 128);
    }
    #[test]
    fn opaque_tokens_accept_all_uint32_bits_and_reject_bad_envelopes() {
        let request = block_token_getter(58).unwrap();
        assert_eq!(unframe(&request).unwrap().1[..6], [4, 1, 58, 0, 0, 0]);
        for value in [0, u32::MAX, 0x7fc00000, 0x80000000] {
            assert_eq!(
                block_token_readback(&editor_reply(&request, value, ""), 58).unwrap(),
                BlockToken(value)
            );
        }
        assert!(block_token_readback(&editor_reply(&request, 5, ""), 59).is_err());
        assert!(block_token_readback(&editor_reply(&request, 5, "unexpected"), 58).is_err());
    }
}
