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

/// Fixed active-channel getter; this API cannot construct a setter packet.
pub fn amp_getter(effect: u16, parameter: u16) -> Result<Vec<u8>, String> {
    if !(58..=61).contains(&effect) || ![0, 1, 2, 3, 4, 5, 20, 21].contains(&parameter) {
        return Err("Unsupported amp getter".into());
    }
    let mut p = vec![0; 15];
    p[0] = 0x1a;
    p[2] = (effect & 127) as u8;
    p[3] = (effect >> 7) as u8;
    p[4] = (parameter & 127) as u8;
    p[5] = (parameter >> 7) as u8;
    frame(1, &p)
}
pub fn amp_value(bytes: &[u8], effect: u16, parameter: u16) -> Result<(u32, f64), String> {
    let (bits, value, _) = amp_readback(bytes, effect, parameter)?;
    value
        .map(|value| (bits, value))
        .ok_or_else(|| "Amp parameter is text-valued".into())
}
pub fn amp_readback(
    bytes: &[u8],
    effect: u16,
    parameter: u16,
) -> Result<(u32, Option<f64>, String), String> {
    let (op, p) = unframe(bytes)?;
    let request = amp_getter(effect, parameter)?;
    let (_, expected) = unframe(&request)?;
    if op != 1 || p.len() < 15 || p[..6] != expected[..6] || p[11..13] != [0, 0] || p[10] > 15 {
        return Err("Amp getter reply mismatch".into());
    }
    let bits = p[6..11]
        .iter()
        .enumerate()
        .fold(0_u64, |a, (i, b)| a | ((*b as u64) << (7 * i))) as u32;
    let raw = f32::from_bits(bits) as f64;
    let length = p[13] as usize + ((p[14] as usize) << 7);
    let data = unpack(&p[15..], length)?;
    let display = String::from_utf8_lossy(data.split(|b| *b == 0).next().unwrap_or_default())
        .trim()
        .to_string();
    if !raw.is_finite() || display.is_empty() {
        return Err("Amp getter returned an invalid value".into());
    }
    if [1, 2, 3, 4, 5, 20].contains(&parameter) {
        let value = display
            .parse::<f64>()
            .map_err(|_| "Amp display conversion unavailable")?;
        if !(0.0..=1.0).contains(&raw) || !value.is_finite() || (value - raw * 10.).abs() >= 0.02 {
            return Err("Amp raw/display conversion does not match the verified profile".into());
        }
        Ok((bits, Some(raw * 10.), display))
    } else if [0, 21].contains(&parameter) {
        Ok((bits, None, display))
    } else {
        Err("Unsupported amp getter parameter".into())
    }
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
    #[test]
    fn amp_read_catalog_is_explicitly_allowlisted() {
        assert!(amp_getter(58, 0).is_ok());
        assert!(amp_getter(58, 21).is_ok());
        assert!(amp_getter(58, 22).is_err());
        assert!(amp_getter(62, 1).is_err());
    }
    #[test]
    fn text_valued_amp_getters_keep_device_text() {
        fn pack(bytes: &[u8]) -> Vec<u8> {
            let mut output = vec![];
            let (mut acc, mut bits) = (0_u16, 0);
            for byte in bytes {
                acc = (acc << 8) | *byte as u16;
                bits += 8;
                while bits >= 7 {
                    bits -= 7;
                    output.push(((acc >> bits) & 127) as u8);
                    acc &= (1 << bits) - 1;
                }
            }
            if bits > 0 {
                output.push(((acc << (7 - bits)) & 127) as u8);
            }
            output
        }
        fn reply(parameter: u8, display: &str) -> Vec<u8> {
            let text = display.as_bytes();
            let mut payload = vec![0x1a, 0, 58, 0, parameter, 0];
            payload.extend([0; 5]);
            payload.extend([0, 0, text.len() as u8, 0]);
            payload.extend(pack(text));
            frame(1, &payload).unwrap()
        }
        let (_, model, name) = amp_readback(&reply(0, "USA Clean"), 58, 0).unwrap();
        assert_eq!(model, None);
        assert_eq!(name, "USA Clean");
        let (_, level, value) = amp_readback(&reply(21, "-3.0 dB"), 58, 21).unwrap();
        assert_eq!(level, None);
        assert_eq!(value, "-3.0 dB");
    }
}
