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
                && (op != 1 || (p.len() > 15 && p.starts_with(&[0x2e, 0])))
                && (op != 0x0c || p != [127])
            {
                return Ok(bytes);
            }
        }
        Err("Read timed out".into())
    };
    let first = decode_grid(&query(grid_query(), 1)?)?;
    let scene = read_scene(&query(frame(0x0c, &[127])?, 0x0c)?)?;
    let second = decode_grid(&query(grid_query(), 1)?)?;
    let scene_after = read_scene(&query(frame(0x0c, &[127])?, 0x0c)?)?;
    if first.slot != second.slot
        || first.name != second.name
        || first.cells != second.cells
        || scene != scene_after
    {
        return Err("Device context changed during capture; retry".into());
    }
    snapshot(second, scene)
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
            let family = if c.instance == 0 {
                "wire"
            } else if (1024..1088).contains(&c.instance) {
                "shunt"
            } else {
                "unknown"
            };
            let label = if family == "unknown" {
                format!("Effect ID {}", c.instance)
            } else {
                family.to_string()
            };
            blocks.push(Block {
                id: id.clone(),
                family: family.into(),
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
        assert!(s.blocks[0].parameters.is_empty());
        assert_eq!(s.edges.len(), 1);
        assert_eq!(s.context.channel, None);
    }
    #[test]
    fn scene_query_echo_rejected() {
        assert!(read_scene(&frame(0x0c, &[127]).unwrap()).is_err());
    }
}
