//! Read-only MIDI capture. The public API exposes no send/write operation.
use crate::protocol::{Grid, decode_grid, frame, grid_query, unframe};
use axe_domain::*;
use midir::{Ignore, MidiInput, MidiOutput};
use std::{
    collections::{BTreeMap, BTreeSet},
    sync::{OnceLock, mpsc},
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};
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
pub fn endpoints() -> Result<Endpoints, String> {
    let input = MidiInput::new("Axe Studio read-only discovery").map_err(|e| e.to_string())?;
    let output = MidiOutput::new("Axe Studio read-only discovery").map_err(|e| e.to_string())?;
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
}
/// A bounded capture owns both endpoints for its duration. Queues and queries are bounded.
/// Writes remain unavailable; no mutating packet can be supplied by callers.
pub fn capture(input_name: &str, output_name: &str) -> Result<Snapshot, String> {
    let mut input = MidiInput::new("Axe Studio capture").map_err(|e| e.to_string())?;
    input.ignore(Ignore::None);
    let output = MidiOutput::new("Axe Studio capture").map_err(|e| e.to_string())?;
    let ins = input
        .ports()
        .into_iter()
        .filter(|p| input.port_name(p).ok().as_deref() == Some(input_name))
        .collect::<Vec<_>>();
    let outs = output
        .ports()
        .into_iter()
        .filter(|p| output.port_name(p).ok().as_deref() == Some(output_name))
        .collect::<Vec<_>>();
    if ins.len() != 1 || outs.len() != 1 {
        return Err("Select unique MIDI input/output endpoints".into());
    }
    let (tx, rx) = mpsc::sync_channel::<Vec<u8>>(64);
    let _connection = input
        .connect(
            &ins[0],
            "Axe Studio read-only",
            move |_, bytes, _| {
                if bytes.len() <= 4096 {
                    let _ = tx.try_send(bytes.to_vec());
                }
            },
            (),
        )
        .map_err(|e| e.to_string())?;
    let mut output = output
        .connect(&outs[0], "Axe Studio read-only")
        .map_err(|e| e.to_string())?;
    let mut query = |packet: Vec<u8>, opcode: u8, timeout: Duration| -> Result<Vec<u8>, String> {
        while rx.try_recv().is_ok() {}
        output.send(&packet).map_err(|e| e.to_string())?;
        let end = Instant::now() + timeout;
        while let Some(remaining) = end.checked_duration_since(Instant::now()) {
            let bytes = rx
                .recv_timeout(remaining)
                .map_err(|_| "Read timed out; no write was sent".to_string())?;
            if let Ok((op, p)) = unframe(&bytes)
                && op == opcode
                && (op != 1 || (p.len() >= 15 && p[..6] == unframe(&packet)?.1[..6]))
                && bytes != packet
                && (op != 0x0c || p != [127])
            {
                return Ok(bytes);
            }
        }
        Err("Read timed out".into())
    };
    let standard_timeout = Duration::from_secs(3);
    let first = decode_grid(&query(grid_query(), 1, standard_timeout)?)?;
    let scene = read_scene(&query(frame(0x0c, &[127])?, 0x0c, standard_timeout)?)?;
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
            let result = query(
                crate::protocol::parameter_getter(effect, id)?,
                1,
                Duration::from_millis(250),
            )
            .and_then(|reply| {
                crate::protocol::parameter_readback(&reply, effect, id)
                    .map(|decoded| (reply, decoded))
            });
            let (reply, decoded, read_error) = match result {
                Ok((reply, decoded)) => (Some(reply), Some(decoded), None),
                Err(error) => (None, None, Some(error)),
            };
            let (raw_bits, raw_numeric, display_text) = decoded
                .map(|(bits, raw, text)| {
                    (Some(bits), Some(raw), (!text.is_empty()).then_some(text))
                })
                .unwrap_or((None, None, None));
            let amp_verified =
                (58..=61).contains(&effect) && [0, 1, 2, 3, 4, 5, 20, 21].contains(&id);
            let converted = if amp_verified {
                reply.as_ref().and_then(|r| {
                    crate::protocol::amp_readback(r, effect, id)
                        .ok()
                        .and_then(|(_, value, _)| value)
                })
            } else {
                None
            };
            let conversion_error = (reply.is_some()
                && amp_verified
                && [1, 2, 3, 4, 5, 20].contains(&id)
                && converted.is_none())
            .then(|| "Device reply did not match the verified amp knob conversion".into());
            let read_status = if raw_bits.is_none() {
                "unavailable"
            } else if conversion_error.is_some() {
                "conversion-unverified"
            } else if amp_verified {
                "verified-device-read"
            } else {
                "device-reply-unverified-getter"
            };
            let error = read_error.or(conversion_error);
            let unit = if converted.is_some() {
                "knob"
            } else if raw_numeric.is_some() {
                "device raw"
            } else {
                "device text"
            };
            block_reads
                .entry(effect)
                .or_insert_with(Vec::new)
                .push(Parameter {
                    id: id.to_string(),
                    label: name,
                    unit: unit.into(),
                    confirmed: converted,
                    display_text,
                    raw_numeric,
                    read_status: Some(read_status.into()),
                    read_error: error,
                    range: converted.map(|_| (0., 10.)),
                    writable: false,
                    automated: None,
                    raw_bits,
                });
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
}
