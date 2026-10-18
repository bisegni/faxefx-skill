//! Read-only MIDI capture. The public API exposes no send/write operation.
use crate::protocol::{Grid, decode_grid, frame, grid_query, unframe};
use axe_domain::*;
use midir::{Ignore, MidiInput, MidiOutput};
use std::{
    collections::{BTreeMap, BTreeSet},
    sync::mpsc,
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};
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
    let mut query = |packet: Vec<u8>, opcode: u8| -> Result<Vec<u8>, String> {
        while rx.try_recv().is_ok() {}
        output.send(&packet).map_err(|e| e.to_string())?;
        let end = Instant::now() + Duration::from_secs(3);
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
    let first = decode_grid(&query(grid_query(), 1)?)?;
    let scene = read_scene(&query(frame(0x0c, &[127])?, 0x0c)?)?;
    let status_before = query(frame(0x13, &[])?, 0x13)?;
    let (_, status) = unframe(&status_before)?;
    if !status.len().is_multiple_of(3) {
        return Err("Invalid block status records".into());
    }
    let mut amps = BTreeMap::new();
    for record in status.as_chunks::<3>().0 {
        let effect = record[0] as u16 + ((record[1] as u16) << 7);
        if (58..=61).contains(&effect) {
            let channel = (record[2] >> 1) & 7;
            let count = (record[2] >> 4) & 7;
            if channel >= count.max(1) || count > 4 {
                return Err("Invalid amp channel status".into());
            }
            let mut parameters = vec![];
            for (id, name) in [
                (0, "model"),
                (1, "gain"),
                (2, "bass"),
                (3, "mid"),
                (4, "treble"),
                (5, "master"),
                (20, "presence"),
                (21, "level"),
            ] {
                let value = query(crate::protocol::amp_getter(effect, id)?, 1)
                    .and_then(|reply| crate::protocol::amp_readback(&reply, effect, id))
                    .ok();
                parameters.push(Parameter {
                    id: name.into(),
                    label: match name {
                        "model" => "Model".into(),
                        "level" => "Level".into(),
                        _ => name.chars().next().unwrap().to_uppercase().to_string() + &name[1..],
                    },
                    unit: if [1, 2, 3, 4, 5, 20].contains(&id) {
                        "knob".into()
                    } else {
                        "device text".into()
                    },
                    confirmed: value.as_ref().and_then(|(_, value, _)| *value),
                    display_text: value.as_ref().map(|(_, _, text)| text.clone()),
                    range: value
                        .as_ref()
                        .and_then(|(_, numeric, _)| numeric.map(|_| (0., 10.))),
                    writable: false,
                    automated: None,
                    raw_bits: value.map(|(bits, _, _)| bits),
                });
            }
            amps.insert(effect, (channel, parameters));
        }
    }
    let status_after = query(frame(0x13, &[])?, 0x13)?;
    if status_before != status_after {
        return Err("Block/channel context changed during capture".into());
    }
    let second = decode_grid(&query(grid_query(), 1)?)?;
    let scene_after = read_scene(&query(frame(0x0c, &[127])?, 0x0c)?)?;
    if first.slot != second.slot
        || first.name != second.name
        || first.cells != second.cells
        || scene != scene_after
    {
        return Err("Device context changed during capture; retry".into());
    }
    let placements = second
        .cells
        .iter()
        .map(|c| ((c.column - 1, c.row - 1), c.instance))
        .collect::<BTreeMap<_, _>>();
    let mut snapshot = snapshot(second, scene)?;
    for block in &mut snapshot.blocks {
        if let Some(effect) = placements.get(&block.position)
            && let Some((_, parameters)) = amps.get(effect)
        {
            block.family = "amp".into();
            block.label = format!("Amplifier {}", effect - 57);
            block.parameters = parameters.clone();
        }
    }
    if amps.len() == 1 {
        snapshot.context.channel = amps.values().next().map(|(channel, _)| *channel);
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
            let (family, type_name, label) = block_identity(c.instance);
            blocks.push(Block {
                id: id.clone(),
                family: family.into(),
                type_name,
                instance_id: (c.instance != 0).then_some(c.instance),
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

/// Map only the instance IDs present in the application's authored identity
/// catalog. Unrecognized IDs remain explicitly unknown; this does not infer a
/// model or parameter schema.
fn block_identity(instance: u16) -> (&'static str, Option<String>, String) {
    let (family, name, short_name, ordinal) = match instance {
        0 => return ("wire", Some("Connection".into()), "WIRE".into()),
        1024..=1087 => return ("shunt", Some("Shunt".into()), "SHUNT".into()),
        46..=49 => ("compressor", "Compressor", "COMP", Some(instance - 45)),
        58..=61 => ("amp", "Amplifier", "AMP", Some(instance - 57)),
        62..=65 => ("cab", "Cabinet", "CAB", Some(instance - 61)),
        66..=69 => ("reverb", "Reverb", "REV", Some(instance - 65)),
        70..=73 => ("delay", "Delay", "DELAY", Some(instance - 69)),
        78..=81 => ("chorus", "Chorus", "CHORUS", Some(instance - 77)),
        118..=121 => ("drive", "Drive", "DRIVE", Some(instance - 117)),
        37..=40 => ("input", "Input", "IN", Some(instance - 36)),
        41 => ("input", "USB Input", "USB IN", None),
        42..=45 => ("output", "Output", "OUT", Some(instance - 41)),
        206..=209 => ("nam-player", "NAM Player", "NAM", Some(instance - 205)),
        _ => return ("unknown", None, format!("ID {instance}")),
    };
    let display = ordinal
        .map(|n| format!("{short_name} {n}"))
        .unwrap_or_else(|| short_name.to_string());
    let type_name = ordinal
        .map(|_| name.to_string())
        .unwrap_or_else(|| name.to_string());
    (family, Some(type_name), display)
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
    fn scene_query_echo_rejected() {
        assert!(read_scene(&frame(0x0c, &[127]).unwrap()).is_err());
    }
}
