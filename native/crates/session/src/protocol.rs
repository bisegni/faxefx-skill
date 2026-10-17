//! Read-only framing ported from codec.py/grid.py. No setters exposed.
const HEADER: &[u8] = &[0xf0, 0, 1, 0x74, 0x10];
pub fn frame(command: u8, payload: &[u8]) -> Result<Vec<u8>, String> {
    if command > 127 || payload.iter().any(|b| *b > 127) {
        return Err("Non-seven-bit data".into());
    }
    let mut result = HEADER.to_vec();
    result.push(command);
    result.extend(payload);
    let sum = result.iter().fold(0, |a, b| a ^ b) & 127;
    result.extend([sum, 247]);
    Ok(result)
}
pub fn unframe(bytes: &[u8]) -> Result<(u8, &[u8]), String> {
    if bytes.len() < 8
        || !bytes.starts_with(HEADER)
        || bytes.last() != Some(&247)
        || bytes[1..bytes.len() - 1].iter().any(|b| *b > 127)
    {
        return Err("Invalid Axe-Fx III frame".into());
    }
    if bytes[..bytes.len() - 2].iter().fold(0, |a, b| a ^ b) & 127 != bytes[bytes.len() - 2] {
        return Err("Invalid checksum".into());
    }
    Ok((bytes[5], &bytes[6..bytes.len() - 2]))
}
pub fn grid_query() -> Vec<u8> {
    let mut p = vec![0; 15];
    p[0] = 0x2e;
    frame(1, &p).expect("static query")
}
pub fn unpack(data: &[u8], length: usize) -> Result<Vec<u8>, String> {
    if data.len() != length.saturating_mul(8).div_ceil(7) || data.iter().any(|b| *b > 127) {
        return Err("Invalid packed bytes".into());
    }
    let mut out = Vec::with_capacity(length);
    let (mut acc, mut bits) = (0_u16, 0);
    for b in data {
        acc = (acc << 7) | *b as u16;
        bits += 7;
        if bits >= 8 {
            bits -= 8;
            out.push((acc >> bits) as u8);
            acc &= (1 << bits) - 1;
        }
    }
    if out.len() != length || acc != 0 {
        return Err("Invalid packed padding".into());
    }
    Ok(out)
}
#[derive(Debug, PartialEq)]
pub struct Cell {
    pub row: u8,
    pub column: u8,
    pub instance: u16,
    pub inputs: u16,
}
#[derive(Debug)]
pub struct Grid {
    pub slot: u16,
    pub name: String,
    pub cells: Vec<Cell>,
}
pub fn decode_grid(bytes: &[u8]) -> Result<Grid, String> {
    let (op, p) = unframe(bytes)?;
    if op != 1 || p.len() < 15 || p[..6] != [0x2e, 0, 0, 0, 0, 0] || p[11..13] != [0, 0] {
        return Err("Unexpected grid envelope".into());
    }
    let length = p[13] as usize + ((p[14] as usize) << 7);
    if length != 640 {
        return Err("Unsupported grid payload layout".into());
    }
    // The common value field is uint32 even though its wire allocation is 35 bits.
    if p[10] > 15 {
        return Err("Oversized value field".into());
    }
    let data = unpack(&p[15..], length)?;
    let slot = u32::from_le_bytes(data[..4].try_into().unwrap());
    if slot > 1023 {
        return Err("Invalid preset slot".into());
    }
    let name = data[16..48].split(|b| *b == 0).next().unwrap();
    let name = String::from_utf8_lossy(name).trim_end().to_owned();
    let mut cells = Vec::with_capacity(84);
    for col in 0..14 {
        for row in 0..6 {
            let i = 304 + (col * 6 + row) * 4;
            let instance = u16::from_le_bytes([data[i], data[i + 1]]);
            let inputs = u16::from_le_bytes([data[i + 2], data[i + 3]]);
            if inputs >= 64 || (col == 0 && inputs != 0) {
                return Err("Invalid input mask".into());
            }
            cells.push(Cell {
                row: row as u8 + 1,
                column: col as u8 + 1,
                instance,
                inputs,
            });
        }
    }
    Ok(Grid {
        slot: slot as u16,
        name,
        cells,
    })
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn fixed_query_parity() {
        assert_eq!(
            frame(0x0c, &[127]).unwrap(),
            vec![240, 0, 1, 116, 16, 12, 127, 102, 247]
        );
        assert_eq!(grid_query().len(), 23);
    }
    #[test]
    fn checksum_rejected() {
        let mut f = grid_query();
        f[6] ^= 1;
        assert!(unframe(&f).is_err());
    }
    #[test]
    fn seven_bit_unpack() {
        assert_eq!(unpack(&[127, 64], 1).unwrap(), vec![255]);
        assert!(unpack(&[127, 65], 1).is_err());
    }
}
