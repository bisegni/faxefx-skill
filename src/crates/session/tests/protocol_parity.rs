//! Cross-language golden fixture contains authored bytes only, never a capture.
use axe_session::protocol::decode_grid;
#[test]
fn authored_legacy_codec_grid_parity() {
    let fixture: serde_json::Value =
        serde_json::from_str(include_str!("fixtures/authored-grid.json")).unwrap();
    let hex = fixture["frame_hex"].as_str().unwrap();
    let bytes = (0..hex.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&hex[i..i + 2], 16).unwrap())
        .collect::<Vec<_>>();
    let grid = decode_grid(&bytes).unwrap();
    assert_eq!(grid.slot, 17);
    assert_eq!(grid.name, "Authored");
    assert_eq!(grid.cells.len(), 84);
    assert_eq!(grid.cells[0].instance, 987);
    assert_eq!(grid.cells[6].instance, 1024);
    assert_eq!(grid.cells[6].inputs, 1);
    let mut corrupt = bytes;
    corrupt[20] ^= 1;
    assert!(decode_grid(&corrupt).is_err());
}

#[test]
fn authored_amp_reply_parity_and_conversion_guard() {
    let fixture: serde_json::Value =
        serde_json::from_str(include_str!("fixtures/authored-amp.json")).unwrap();
    let decode = |name: &str| {
        let hex = fixture[name].as_str().unwrap();
        (0..hex.len())
            .step_by(2)
            .map(|i| u8::from_str_radix(&hex[i..i + 2], 16).unwrap())
            .collect::<Vec<_>>()
    };
    let (bits, value) = axe_session::protocol::amp_value(&decode("valid"), 58, 1).unwrap();
    assert_eq!(bits, 0x3f000000);
    assert_eq!(value, 5.);
    assert!(axe_session::protocol::amp_value(&decode("mismatch"), 58, 1).is_err());
    assert!(axe_session::protocol::amp_getter(62, 1).is_err());
    assert!(axe_session::protocol::amp_getter(58, 999).is_err());
}
