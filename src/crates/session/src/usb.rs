//! MIDI capture and explicit, typed user-operated device commands.
pub mod live;
mod store;
use crate::protocol::{Grid, decode_grid, frame, grid_query, unframe};
use axe_domain::*;
use midir::{Ignore, MidiInput, MidiOutput};
use std::{
    collections::{BTreeMap, BTreeSet},
    sync::{OnceLock, mpsc},
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};
pub use store::{StoreWire, store_preset, store_preset_with};
const EFFECT_IDENTITIES: &str = include_str!("../../../resources/axe-effects/identities.json");
fn catalog() -> &'static serde_json::Value {
    static CATALOG: OnceLock<serde_json::Value> = OnceLock::new();
    CATALOG.get_or_init(|| serde_json::from_str(EFFECT_IDENTITIES).expect("bundled effect catalog"))
}
pub fn catalog_info() -> serde_json::Value {
    let catalog = catalog();
    serde_json::json!({
        "provenance":catalog["provenance"],
        "known_block_instances":catalog["instances"].as_array().map_or(0, Vec::len),
        "known_parameter_families":catalog["types"].as_array().map_or(0, Vec::len),
        "generic_getter_hardware_verified":false,
        "values":"Matching device replies are preserved as display text and unconverted raw floats. Only the eight amp getters have verified getter semantics; other family getters need physical validation."
    })
}
#[derive(Debug, Clone)]
pub struct Endpoints {
    pub inputs: Vec<String>,
    pub outputs: Vec<String>,
}
type DiscoveryReply = mpsc::SyncSender<Result<Endpoints, String>>;

fn discovery_loop(
    requests: mpsc::Receiver<DiscoveryReply>,
    mut scan: impl FnMut() -> Result<Endpoints, String>,
    mut pump: impl FnMut(),
) {
    loop {
        pump();
        match requests.recv_timeout(Duration::from_millis(50)) {
            Ok(reply) => {
                let _ = reply.send(scan());
            }
            Err(mpsc::RecvTimeoutError::Timeout) => {}
            Err(mpsc::RecvTimeoutError::Disconnected) => break,
        }
    }
}

/// Discovery has a stable thread/client lifetime. On macOS its CoreFoundation
/// run loop is serviced between scans so CoreMIDI can process hot-plug changes.
/// No endpoint is opened and no MIDI packet is sent by this service.
pub fn endpoints() -> Result<Endpoints, String> {
    static REQUESTS: OnceLock<Result<mpsc::Sender<DiscoveryReply>, String>> = OnceLock::new();
    let requests = REQUESTS
        .get_or_init(|| {
            let (tx, rx) = mpsc::channel::<DiscoveryReply>();
            std::thread::Builder::new()
                .name("axe-midi-discovery".into())
                .spawn(move || {
                    let mut clients = None;
                    discovery_loop(
                        rx,
                        || {
                            if clients.is_none() {
                                clients = Some((
                                    MidiInput::new("Axe Studio discovery")
                                        .map_err(|e| e.to_string())?,
                                    MidiOutput::new("Axe Studio discovery")
                                        .map_err(|e| e.to_string())?,
                                ));
                            }
                            let (input, output) = clients.as_ref().unwrap();
                            Ok(Endpoints {
                                inputs: input
                                    .ports()
                                    .iter()
                                    .map(|p| input.port_name(p).map_err(|e| e.to_string()))
                                    .collect::<Result<_, _>>()?,
                                outputs: output
                                    .ports()
                                    .iter()
                                    .map(|p| output.port_name(p).map_err(|e| e.to_string()))
                                    .collect::<Result<_, _>>()?,
                            })
                        },
                        service_discovery_run_loop,
                    );
                })
                .map(|_| tx)
                .map_err(|e| e.to_string())
        })
        .as_ref()
        .map_err(Clone::clone)?;
    let (tx, rx) = mpsc::sync_channel(1);
    requests
        .send(tx)
        .map_err(|_| "MIDI discovery service stopped")?;
    rx.recv_timeout(Duration::from_secs(5))
        .map_err(|_| "MIDI discovery timed out".to_string())?
}

#[cfg(target_os = "macos")]
fn service_discovery_run_loop() {
    use std::ffi::c_void;
    #[link(name = "CoreFoundation", kind = "framework")]
    unsafe extern "C" {
        static kCFRunLoopDefaultMode: *const c_void;
        fn CFRunLoopRunInMode(mode: *const c_void, seconds: f64, return_after_source: u8) -> i32;
    }
    // CoreFoundation owns the constant; this is always called on the same
    // discovery thread that owns the MIDI clients, never on the UI thread.
    unsafe {
        CFRunLoopRunInMode(kCFRunLoopDefaultMode, 0.001, 0);
    }
}
#[cfg(not(target_os = "macos"))]
fn service_discovery_run_loop() {}
/// A bounded capture owns both endpoints for its duration. Queues and queries are bounded.
/// Writes remain unavailable; no mutating packet can be supplied by callers.
pub fn capture(input_name: &str, output_name: &str) -> Result<Snapshot, String> {
    let mut wire = store::MidiStoreWire::connect(input_name, output_name)?;
    capture_with(&mut wire)
}

pub fn capture_with(wire: &mut impl StoreWire) -> Result<Snapshot, String> {
    let mut query = |packet: Vec<u8>, _opcode: u8, _timeout: Duration| wire.query(&packet);
    let standard_timeout = Duration::from_secs(3);
    let first = decode_grid(&query(grid_query(), 1, standard_timeout)?)?;
    let scene = read_scene(&query(frame(0x0c, &[127])?, 0x0c, standard_timeout)?)?;
    let mut scene_names = Vec::with_capacity(8);
    for index in 0..8 {
        scene_names.push(read_scene_name(
            &query(frame(0x0e, &[index])?, 0x0e, standard_timeout)?,
            index,
        )?);
    }
    let status_before = query(frame(0x13, &[])?, 0x13, standard_timeout)?;
    let (_, status) = unframe(&status_before)?;
    if !status.len().is_multiple_of(3) {
        return Err("Invalid block status records".into());
    }
    let placed = first
        .cells
        .iter()
        .filter_map(|cell| (cell.instance != 0).then_some(cell.instance))
        .collect::<BTreeSet<_>>();
    let mut block_reads = BTreeMap::new();
    let mut status_state = BTreeMap::new();
    for record in status.as_chunks::<3>().0 {
        let effect = record[0] as u16 + ((record[1] as u16) << 7);
        let channel = (record[2] >> 1) & 7;
        let count = (record[2] >> 4) & 7;
        if channel >= count.max(1) || count > 4 {
            return Err("Invalid block channel status".into());
        }
        status_state.insert(effect, (channel, record[2] & 1 != 0));
        if !placed.contains(&effect) {
            continue;
        }
        let (_, _, _, prefix) = block_identity(effect);
        let Some(prefix) = prefix else { continue };
        for (id, name) in parameter_specs(&prefix) {
            block_reads
                .entry(effect)
                .or_insert_with(Vec::new)
                .push(live::unavailable_parameter(id, name));
        }
    }
    let status_after = query(frame(0x13, &[])?, 0x13, standard_timeout)?;
    if status_before != status_after {
        return Err("Block/channel context changed during capture".into());
    }
    let second = decode_grid(&query(grid_query(), 1, standard_timeout)?)?;
    let scene_after = read_scene(&query(frame(0x0c, &[127])?, 0x0c, standard_timeout)?)?;
    if first.slot != second.slot
        || first.name != second.name
        || first.cells != second.cells
        || scene != scene_after
    {
        return Err("Device context changed during capture; retry".into());
    }
    let mut snapshot = snapshot(second, scene)?;
    snapshot.scene_names = scene_names;
    for block in &mut snapshot.blocks {
        if let Some(effect) = block.instance_id {
            if let Some((channel, bypassed)) = status_state.get(&effect) {
                block.channel = Some(*channel);
                block.bypassed = Some(*bypassed);
            }
            if let Some(parameters) = block_reads.remove(&effect) {
                block.parameters = parameters;
            }
        }
    }
    let amp_status = status_state
        .iter()
        .filter(|(id, _)| (58..=61).contains(*id))
        .map(|(_, value)| *value)
        .collect::<Vec<_>>();
    if let [(channel, _)] = amp_status.as_slice() {
        snapshot.context.channel = Some(*channel);
    }
    Ok(snapshot)
}
fn read_scene(bytes: &[u8]) -> Result<u8, String> {
    let (op, p) = unframe(bytes)?;
    if op != 0x0c || p.len() != 1 || p[0] > 7 {
        return Err("Invalid scene reply".into());
    }
    Ok(p[0])
}
fn read_scene_name(bytes: &[u8], expected: u8) -> Result<String, String> {
    let (op, payload) = unframe(bytes)?;
    if op != 0x0e || payload.len() < 2 || payload[0] != expected {
        return Err("Invalid scene-name reply".into());
    }
    let name = payload[1..].split(|b| *b == 0).next().unwrap_or_default();
    Ok(String::from_utf8_lossy(name).trim_end().to_owned())
}

/// Select one of the eight documented scenes and require a matching readback.
/// This function is deliberately separate from parameter-write capability.
pub fn select_scene(input_name: &str, output_name: &str, scene: u8) -> Result<(), String> {
    if scene > 7 {
        return Err("Scene must be 1–8".into());
    }
    let mut wire = store::MidiStoreWire::connect(input_name, output_name)?;
    select_scene_with(&mut wire, scene)
}

pub fn select_scene_with(wire: &mut impl StoreWire, scene: u8) -> Result<(), String> {
    if scene > 7 {
        return Err("Scene must be 1–8".into());
    }
    wire.send_once(&frame(0x0c, &[scene])?)?;
    std::thread::sleep(Duration::from_millis(100));
    let actual = read_scene(&wire.query(&frame(0x0c, &[127])?)?)?;
    if actual != scene {
        return Err("Scene selection was not confirmed".into());
    }
    Ok(())
}

fn snapshot(grid: Grid, scene: u8) -> Result<Snapshot, String> {
    let mut used = BTreeSet::new();
    let mut occupied = BTreeSet::new();
    for c in &grid.cells {
        if c.instance != 0 || c.inputs != 0 {
            used.insert((c.column, c.row));
        }
        for row in 1..=6 {
            if c.inputs & (1 << (row - 1)) != 0 {
                used.insert((c.column - 1, row));
            }
        }
        if c.instance != 0 && !occupied.insert(c.instance) {
            return Err("Duplicate grid instance".into());
        }
    }
    let ids = used
        .iter()
        .map(|(col, row)| ((*col, *row), format!("cell-r{row}c{col}")))
        .collect::<BTreeMap<_, _>>();
    let mut blocks = vec![];
    let mut edges = vec![];
    for c in &grid.cells {
        if let Some(id) = ids.get(&(c.column, c.row)) {
            let (family, type_name, label, parameter_prefix) = block_identity(c.instance);
            blocks.push(Block {
                id: id.clone(),
                family: family.into(),
                type_name,
                instance_id: (c.instance != 0).then_some(c.instance),
                parameter_prefix,
                channel: None,
                bypassed: None,
                label,
                position: (c.column - 1, c.row - 1),
                parameters: vec![],
            });
            for row in 1..=6 {
                if c.inputs & (1 << (row - 1)) != 0 {
                    edges.push((ids[&(c.column - 1, row)].clone(), id.clone()));
                }
            }
        }
    }
    let identity = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|e| e.to_string())?
        .as_nanos()
        .to_string();
    Ok(Snapshot {
        context: Context {
            session: format!("usb-{identity}"),
            generation: 1,
            preset: grid.slot,
            scene: Some(scene),
            channel: None,
            target: Target::Hardware,
        },
        name: grid.name,
        scene_names: Vec::new(),
        blocks,
        edges,
    })
}

/// Resolve names and parameter identifiers from static Axe-Edit symbol
/// metadata. This does not establish that the connected firmware uses it.
fn block_identity(instance: u16) -> (&'static str, Option<String>, String, Option<String>) {
    if instance == 0 {
        return ("wire", Some("Connection".into()), "WIRE".into(), None);
    }
    if (1024..=1087).contains(&instance) {
        return ("shunt", Some("Shunt".into()), "SHUNT".into(), None);
    }
    if let Some(item) = catalog()["instances"].as_array().and_then(|items| {
        items
            .iter()
            .find(|item| item["id"].as_u64() == Some(instance.into()))
    }) {
        return (
            item["family"].as_str().unwrap_or("unknown"),
            item["type_name"].as_str().map(str::to_owned),
            item["label"].as_str().unwrap_or("Unknown block").to_owned(),
            item["parameter_prefix"].as_str().map(str::to_owned),
        );
    }
    ("unknown", None, format!("ID {instance}"), None)
}

fn parameter_specs(prefix: &str) -> Vec<(u16, String)> {
    catalog()["types"]
        .as_array()
        .and_then(|types| {
            types
                .iter()
                .find(|item| item["parameter_prefix"].as_str() == Some(prefix))
        })
        .and_then(|item| item["parameters"].as_array())
        .into_iter()
        .flatten()
        .filter_map(|parameter| {
            Some((
                u16::try_from(parameter["id"].as_u64()?).ok()?,
                parameter["name"].as_str()?.to_owned(),
            ))
        })
        .collect()
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn unknown_ids_remain_unknown() {
        let grid = Grid {
            slot: 3,
            name: "Authored".into(),
            cells: vec![
                crate::protocol::Cell {
                    row: 1,
                    column: 1,
                    instance: 987,
                    inputs: 0,
                },
                crate::protocol::Cell {
                    row: 2,
                    column: 2,
                    instance: 1024,
                    inputs: 1,
                },
            ],
        };
        let s = snapshot(grid, 0).unwrap();
        assert_eq!(s.blocks[0].family, "unknown");
        assert_eq!(s.blocks[0].type_name, None);
        assert_eq!(s.blocks[0].label, "ID 987");
        assert!(s.blocks[0].parameters.is_empty());
        assert_eq!(s.edges.len(), 1);
        assert_eq!(s.context.channel, None);
    }
    #[test]
    fn recognized_instance_ids_get_clear_block_names_and_types() {
        let grid = Grid {
            slot: 3,
            name: "Authored".into(),
            cells: vec![crate::protocol::Cell {
                row: 1,
                column: 1,
                instance: 58,
                inputs: 0,
            }],
        };
        let block = snapshot(grid, 0).unwrap().blocks.remove(0);
        assert_eq!(block.family, "amp");
        assert_eq!(block.type_name.as_deref(), Some("Amplifier"));
        assert_eq!(block.instance_id, Some(58));
        assert_eq!(block.label, "AMP 1");
    }
    #[test]
    fn bundled_catalog_covers_multiple_effect_families() {
        let info = catalog_info();
        assert_eq!(info["known_block_instances"], 174);
        assert_eq!(info["known_parameter_families"], 42);
        for prefix in ["DISTORT_", "REVERB_", "DELAY_", "CHORUS_", "CABINET_"] {
            let parameters = parameter_specs(prefix);
            assert!(!parameters.is_empty(), "missing {prefix}");
            assert!(
                parameters
                    .iter()
                    .all(|(id, name)| *id <= 0x3fff && !name.is_empty())
            );
        }
    }
    #[test]
    fn scene_query_echo_rejected() {
        assert!(read_scene(&frame(0x0c, &[127]).unwrap()).is_err());
    }
    #[test]
    fn scene_name_reply_must_match_requested_index() {
        let mut payload = vec![2];
        payload.extend_from_slice(b"Lead\0");
        let reply = frame(0x0e, &payload).unwrap();
        assert_eq!(read_scene_name(&reply, 2).unwrap(), "Lead");
        assert!(read_scene_name(&reply, 1).is_err());
    }
}

#[cfg(test)]
mod discovery_tests {
    use super::*;
    #[test]
    fn persistent_discovery_sees_late_devices_and_recovers_from_scan_error() {
        // No CoreMIDI client or endpoint is opened: only inject catalog changes.
        let (tx, rx) = mpsc::channel();
        let worker = std::thread::spawn(move || {
            let mut scans = 0;
            let mut pumped = false;
            discovery_loop(
                rx,
                || {
                    scans += 1;
                    if scans == 2 {
                        return Err("transient catalog failure".into());
                    }
                    let names = if scans >= 3 {
                        vec!["Axe-Fx III".into()]
                    } else {
                        vec![]
                    };
                    Ok(Endpoints {
                        inputs: names.clone(),
                        outputs: names,
                    })
                },
                || {
                    pumped = true;
                },
            );
            assert!(pumped);
            assert_eq!(scans, 4);
        });
        let scan = || {
            let (reply, result) = mpsc::sync_channel(1);
            tx.send(reply).unwrap();
            result.recv_timeout(Duration::from_secs(1)).unwrap()
        };
        assert!(scan().unwrap().inputs.is_empty());
        assert!(scan().is_err());
        assert_eq!(scan().unwrap().inputs, ["Axe-Fx III"]);
        assert_eq!(scan().unwrap().outputs, ["Axe-Fx III"]);
        drop(tx);
        worker.join().unwrap();
    }
}
