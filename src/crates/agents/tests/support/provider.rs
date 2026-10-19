//! Authored deterministic provider double. Never bundled with the application.
use serde_json::{Value, json};
use std::io::{BufRead, Read, Write};
fn emit(v: Value) {
    println!("{v}");
    std::io::stdout().flush().unwrap();
}
fn main() {
    let args = std::env::args().skip(1).collect::<Vec<_>>();
    if args.iter().any(|a| a == "--contract-test") {
        contract();
        return;
    }
    if args.iter().any(|a| a == "--version") {
        println!("2.1.302 (authored fixture)");
        return;
    }
    if args.iter().any(|a| a == "--print") {
        claude(&args);
        return;
    }
    codex();
}
fn codex() {
    let mut turn = 0;
    for line in std::io::stdin().lock().lines() {
        let line = line.unwrap();
        let v: Value = serde_json::from_str(&line).unwrap();
        let id = v["id"].clone();
        let method = v["method"].as_str().unwrap_or("");
        let result = match method {
            "initialize" => json!({"userAgent":"codex-cli/0.163.0 authored fixture"}),
            "config/read" => {
                json!({"config":{"features":{"shell_tool":false,"unified_exec":false,"code_mode":false,"multi_agent":false,"multi_agent_v2":false,"apps":false,"plugins":false,"hooks":false,"browser_use":false,"computer_use":false,"image_generation":false,"request_permissions_tool":false,"code_mode_host":true}}})
            }
            "account/read" => json!({"requiresOpenaiAuth":false}),
            "skills/list" => json!({"data":[]}),
            "thread/start" | "thread/resume" => {
                assert_eq!(v["params"]["sandbox"], "read-only");
                assert_eq!(v["params"]["dynamicTools"].as_array().unwrap().len(), 8);
                json!({"thread":{"id":"fixture-thread"},"sandbox":{"type":"readOnly"}})
            }
            "turn/start" => {
                turn += 1;
                if turn == 1 {
                    assert_eq!(v["params"]["input"][1]["type"], "skill");
                    assert_eq!(v["params"]["input"][1]["name"], "axefx-live");
                }
                emit(json!({"id":id,"result":{"turn":{"id":format!("turn-{turn}")}}}));
                emit(
                    json!({"method":"item/tool/call","id":900+turn,"params":{"threadId":"fixture-thread","turnId":format!("turn-{turn}"),"callId":format!("tool-{turn}"),"tool":"session_inspect","arguments":{}}}),
                );
                continue;
            }
            "turn/interrupt" => {
                emit(
                    json!({"method":"turn/completed","params":{"threadId":"fixture-thread","turn":{"id":format!("turn-{turn}")}}}),
                );
                json!({})
            }
            _ => {
                if v.get("method").is_none() && v.get("result").is_some() {
                    let text = if turn == 1 {
                        "AXEFX_LIVE_READY_V1"
                    } else {
                        "Authored provider response."
                    };
                    emit(
                        json!({"method":"item/agentMessage/delta","params":{"threadId":"fixture-thread","turnId":format!("turn-{turn}"),"itemId":format!("item-{turn}"),"delta":text}}),
                    );
                    emit(
                        json!({"method":"item/completed","params":{"threadId":"fixture-thread","item":{"type":"agentMessage","id":format!("item-{turn}"),"text":text}}}),
                    );
                    emit(
                        json!({"method":"turn/completed","params":{"threadId":"fixture-thread","turn":{"id":format!("turn-{turn}"),"error":null}}}),
                    );
                }
                continue;
            }
        };
        emit(json!({"id":id,"result":result}));
    }
}
fn claude(args: &[String]) {
    for flag in [
        "--bare",
        "--restricted",
        "--strict-mcp-config",
        "--tools",
        "--setting-sources",
        "--permission-prompts",
    ] {
        assert!(args.iter().any(|a| a == flag), "missing {flag}");
    }
    let at = args.iter().position(|a| a == "--tools").unwrap();
    assert_eq!(args[at + 1], "");
    let at = args.iter().position(|a| a == "--mcp-config").unwrap();
    assert!(!args[at + 1].contains("Bearer"));
    let config: Value = serde_json::from_slice(&std::fs::read(&args[at + 1]).unwrap()).unwrap();
    let config = &config["mcpServers"]["axe_studio"];
    let mut prompt = String::new();
    std::io::stdin().read_to_string(&mut prompt).unwrap();
    emit(
        json!({"type":"system","subtype":"init","session_id":"11111111-1111-4111-8111-111111111111","tools":axe_agents::TOOLS.iter().map(|(n,_)|format!("mcp__axe_studio__{n}")).collect::<Vec<_>>()}),
    );
    let url = config["url"]
        .as_str()
        .unwrap()
        .strip_prefix("http://")
        .unwrap();
    let (addr, path) = url.split_once('/').unwrap();
    let mut stream = std::net::TcpStream::connect(addr).unwrap();
    stream
        .set_read_timeout(Some(std::time::Duration::from_secs(5)))
        .unwrap();
    let body=json!({"jsonrpc":"2.0","id":3,"method":"tools/call","params":{"name":"session_inspect","arguments":{}}}).to_string();
    write!(stream,"POST /{path} HTTP/1.1\r\nHost: {addr}\r\nAuthorization: {}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",config["headers"]["Authorization"].as_str().unwrap(),body.len()).unwrap();
    let mut reply = String::new();
    stream.read_to_string(&mut reply).unwrap();
    assert!(reply.contains("200 OK"));
    let text = if prompt.contains("AXEFX_LIVE_READY_V1") {
        "AXEFX_LIVE_READY_V1"
    } else {
        "Authored provider response."
    };
    emit(
        json!({"type":"stream_event","event":{"type":"content_block_delta","delta":{"type":"text_delta","text":text}}}),
    );
    emit(json!({"type":"result","is_error":false,"result":text}));
}
fn contract() {
    use axe_agents::{
        Provider,
        runtime::{AgentEvent, AgentHandle},
    };
    use std::sync::{Arc, Mutex};
    use std::time::{Duration, Instant};
    for provider in [Provider::Codex, Provider::Claude] {
        let root =
            std::env::temp_dir().join(format!("axe-provider-contract-{}", uuid::Uuid::new_v4()));
        let calls = Arc::new(Mutex::new(vec![]));
        let observed = calls.clone();
        let handle = AgentHandle::connect(
            provider,
            std::env::current_exe().unwrap(),
            root.clone(),
            None,
            Arc::new(move |call, scope| {
                assert!(scope.valid());
                assert_eq!(call.name, "session_inspect");
                observed.lock().unwrap().push(scope.writable());
                Ok(json!({"target":"authored mock"}))
            }),
            Arc::new(|_| Ok(())),
        );
        let deadline = Instant::now() + Duration::from_secs(15);
        loop {
            assert!(
                Instant::now() < deadline,
                "initialization timed out {provider:?}"
            );
            match handle
                .events
                .lock()
                .unwrap()
                .recv_timeout(Duration::from_millis(100))
            {
                Ok(AgentEvent::Ready) => break,
                Ok(AgentEvent::Failed(e)) => panic!("{provider:?}: {e}"),
                _ => {}
            }
        }
        assert_eq!(calls.lock().unwrap().first(), Some(&false));
        handle.submit("Explain the authored mock capture.").unwrap();
        let mut text = String::new();
        loop {
            assert!(Instant::now() < deadline, "turn timed out {provider:?}");
            match handle
                .events
                .lock()
                .unwrap()
                .recv_timeout(Duration::from_millis(100))
            {
                Ok(AgentEvent::Delta { text: t, .. }) => text.push_str(&t),
                Ok(AgentEvent::Completed) => break,
                Ok(AgentEvent::Failed(e)) => panic!("{e}"),
                _ => {}
            }
        }
        assert_eq!(text, "Authored provider response.");
        assert_eq!(calls.lock().unwrap().last(), Some(&true));
        handle.stop();
        drop(handle);
        let _ = std::fs::remove_dir_all(root);
    }
    println!("Both provider contracts passed");
}
