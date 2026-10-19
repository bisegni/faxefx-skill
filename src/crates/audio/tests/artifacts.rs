use axe_audio::artifact::Library;
fn wav() -> Vec<u8> {
    let rate = 48000_u32;
    let count = rate;
    let mut data = Vec::new();
    data.extend(b"RIFF");
    data.extend((36 + count * 4).to_le_bytes());
    data.extend(b"WAVEfmt ");
    data.extend(16_u32.to_le_bytes());
    data.extend(3_u16.to_le_bytes());
    data.extend(1_u16.to_le_bytes());
    data.extend(rate.to_le_bytes());
    data.extend((rate * 4).to_le_bytes());
    data.extend(4_u16.to_le_bytes());
    data.extend(32_u16.to_le_bytes());
    data.extend(b"data");
    data.extend((count * 4).to_le_bytes());
    for i in 0..count {
        let x = 0.25 * (std::f32::consts::TAU * 1000. * i as f32 / rate as f32).sin();
        data.extend(x.to_le_bytes());
    }
    data
}
#[test]
fn authored_artifact_is_local_verified_and_bounded() {
    let root = std::env::temp_dir().join(format!("axe-audio-test-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir_all(&root).unwrap();
    let source = root.join("authored.wav");
    std::fs::write(&source, wav()).unwrap();
    let library = Library::open(root.join("library")).unwrap();
    let a = library.import(&source).unwrap();
    assert_eq!(a.id.len(), 64);
    assert!((a.duration - 1.).abs() < 0.001);
    assert!(a.waveform.len() <= 256);
    let listed = library.list().unwrap();
    assert_eq!(listed.len(), 1);
    assert_eq!(listed[0].id, a.id);
    let shared = library.agent_audio(&a.id, 0.0, 1.0).unwrap();
    assert!(
        shared.starts_with("UklGR"),
        "normalized payload should be WAV data"
    );
    assert!(library.agent_audio(&a.id, 0.0, 31.0).is_err());
    assert!((a.rms - 0.1767767).abs() < 0.001);
    assert!((library.excerpt_rms(&a.id, 0.25, 0.5).unwrap() - a.rms).abs() < 0.001);
    assert!(library.excerpt_rms(&a.id, 2., 1.).is_err());
    let path = library.path(&a.id).unwrap();
    std::fs::write(path, b"changed").unwrap();
    assert!(library.path(&a.id).is_err());
    let _ = std::fs::remove_dir_all(root);
}
#[test]
#[ignore = "Requires AXE_AUDIO_WORKER pointing to a frozen interpreter-bundled worker"]
fn frozen_worker_without_system_python() {
    use std::io::{BufRead, BufReader, Write};
    let worker = std::env::var_os("AXE_AUDIO_WORKER").expect("Set frozen worker path");
    let root = std::env::temp_dir().join(format!("axe-worker-test-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir_all(&root).unwrap();
    let source = root.join("sine.wav");
    std::fs::write(&source, wav()).unwrap();
    let mut child = std::process::Command::new(worker)
        .env_clear()
        .env("PATH", "/not-a-system-python")
        .current_dir(&root)
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .spawn()
        .unwrap();
    let mut input = child.stdin.take().unwrap();
    let mut output = BufReader::new(child.stdout.take().unwrap());
    for (id, method, params) in [
        (1, "initialize", serde_json::json!({"protocol_version":1})),
        (
            2,
            "measure",
            serde_json::json!({"path":source,"artifact":"authored","start":0,"duration":1}),
        ),
    ] {
        writeln!(
            input,
            "{}",
            serde_json::json!({"jsonrpc":"2.0","id":id,"method":method,"params":params})
        )
        .unwrap();
        input.flush().unwrap();
        let mut line = String::new();
        output.read_line(&mut line).unwrap();
        let response: serde_json::Value = serde_json::from_str(&line).unwrap();
        assert!(response.get("error").is_none(), "{response}");
        if id == 2 {
            let centroid = response["result"]["measurements"]["spectral_centroid_hz"]
                .as_f64()
                .unwrap();
            assert!((centroid - 1000.).abs() < 2.);
        }
    }
    drop(input);
    assert!(child.wait().unwrap().success());
    let _ = std::fs::remove_dir_all(root);
}
