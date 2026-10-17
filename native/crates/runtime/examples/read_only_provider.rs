//! Read-only provider smoke check using exclusively authored Test mock data.
use axe_agents::{
    Provider,
    runtime::{AgentEvent, AgentHandle},
};
use axe_session::{Mock, Transport};
use std::time::{Duration, Instant};
fn main() -> Result<(), String> {
    let mutate = std::env::args().any(|a| a == "--mutate-mock");
    let provider = if std::env::args().any(|a| a == "claude") {
        Provider::Claude
    } else {
        Provider::Codex
    };
    let root = std::env::temp_dir().join(format!("axe-readonly-smoke-{}", uuid::Uuid::new_v4()));
    let backend = axe_runtime::Backend::open(root.clone())?;
    backend
        .lock()
        .unwrap()
        .bind(Mock::default().snapshot().map_err(|e| e.to_string())?)?;
    let (tools, persist) = axe_runtime::handlers(backend.clone(), "mock".into(), provider);
    let exe = axe_agents::runtime::executable(match provider {
        Provider::Codex => "codex",
        Provider::Claude => "claude",
    })?;
    let handle = AgentHandle::connect(provider, exe, root.join("provider"), None, tools, persist);
    let deadline = Instant::now() + Duration::from_secs(100);
    let mut submitted = false;
    loop {
        if Instant::now() > deadline {
            handle.stop();
            return Err("Read-only provider smoke timeout".into());
        }
        if let Ok(event) = handle
            .events
            .lock()
            .unwrap()
            .recv_timeout(Duration::from_millis(200))
        {
            match event {
                AgentEvent::Ready => {
                    println!("Skill initialization verified for {provider:?}");
                    handle.submit(if mutate{"Inspect session, then use parameters_set to set amp1 gain to exactly 6.2 in the Test mock. Use idempotency_key smoke-gain-6.2 and report the verified value. No other parameters."}else{"Using only session_inspect, briefly describe the authored Test mock routing. Do not change any parameters."})?;
                    submitted = true;
                }
                AgentEvent::Delta { text, .. } if submitted => print!("{text}"),
                AgentEvent::Completed if submitted => {
                    println!("\nRead-only conversation passed.");
                    break;
                }
                AgentEvent::Failed(e) => return Err(e),
                _ => {}
            }
        }
    }
    let mut b = backend.lock().unwrap();
    if mutate {
        assert!(b.dirty());
        let value = b
            .snapshot
            .as_ref()
            .unwrap()
            .blocks
            .iter()
            .find(|b| b.id == "amp1")
            .unwrap()
            .parameters
            .iter()
            .find(|p| p.id == "gain")
            .unwrap()
            .confirmed;
        assert_eq!(value, Some(6.2));
        b.restore()?;
        assert!(!b.dirty());
        println!("Mock mutation/readback/restoration verified.");
    } else {
        assert!(!b.dirty());
    }
    Ok(())
}
