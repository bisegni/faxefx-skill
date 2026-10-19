//! Authenticated loopback endpoint exposing exactly the app tool allowlist.
use crate::runtime::{HostCall, ToolReply, tool_specs};
use serde_json::{Value, json};
use std::io::Read;
use std::{
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
        mpsc,
    },
    time::Duration,
};
pub struct Mcp {
    pub url: String,
    pub token: String,
    stop: Arc<AtomicBool>,
    pub calls: mpsc::Receiver<(HostCall, mpsc::SyncSender<ToolReply>)>,
}
fn tool_result_content(result: ToolReply) -> Value {
    match result {
        Ok(mut value) => {
            let attachment = value
                .as_object_mut()
                .and_then(|object| object.remove("audio_attachment"));
            let mut content = vec![json!({"type":"text","text":value.to_string()})];
            if let Some(audio) = attachment {
                match (audio["data_base64"].as_str(), audio["mime_type"].as_str()) {
                    (Some(data), Some(mime)) => {
                        content.push(json!({"type":"audio","data":data,"mimeType":mime}))
                    }
                    _ => {
                        return json!({"isError":true,"content":[{"type":"text","text":"Audio attachment payload is invalid"}]});
                    }
                }
            }
            json!({"isError":false,"content":content})
        }
        Err(error) => json!({"isError":true,"content":[{"type":"text","text":error}]}),
    }
}
impl Mcp {
    pub fn start() -> Result<Self, String> {
        let server = tiny_http::Server::http("127.0.0.1:0").map_err(|e| e.to_string())?;
        let url = format!("http://{}/mcp", server.server_addr());
        let token = uuid::Uuid::new_v4().to_string();
        let expected = format!("Bearer {token}");
        let stop = Arc::new(AtomicBool::new(false));
        let stopped = stop.clone();
        let (tx, calls) = mpsc::sync_channel(32);
        std::thread::spawn(move || {
            while !stopped.load(Ordering::SeqCst) {
                let Ok(Some(mut request)) = server.recv_timeout(Duration::from_millis(50)) else {
                    continue;
                };
                let auth = request
                    .headers()
                    .iter()
                    .any(|h| h.field.equiv("Authorization") && h.value.as_str() == expected);
                let origin = request.headers().iter().any(|h| h.field.equiv("Origin"));
                if !auth || origin || request.url() != "/mcp" {
                    let _ = request.respond(tiny_http::Response::empty(403));
                    continue;
                }
                if request.method() != &tiny_http::Method::Post {
                    let _ = request.respond(tiny_http::Response::empty(405));
                    continue;
                }
                if request.body_length().is_some_and(|n| n > 1024 * 1024) {
                    let _ = request.respond(tiny_http::Response::empty(413));
                    continue;
                }
                let mut bytes = vec![];
                if std::io::Read::take(request.as_reader(), 1024 * 1024 + 1)
                    .read_to_end(&mut bytes)
                    .is_err()
                    || bytes.len() > 1024 * 1024
                {
                    let _ = request.respond(tiny_http::Response::empty(413));
                    continue;
                }
                let v = serde_json::from_slice::<Value>(&bytes).unwrap_or(Value::Null);
                let method = v["method"].as_str().unwrap_or("");
                let id = v["id"].clone();
                if id.is_null() && method == "notifications/initialized" {
                    let _ = request.respond(tiny_http::Response::empty(202));
                    continue;
                }
                let result: Result<Value, String> = match method {
                    "initialize" => Ok(
                        json!({"protocolVersion":v["params"]["protocolVersion"].as_str().filter(|v|["2025-03-26","2025-06-18","2025-11-25"].contains(v)).unwrap_or("2025-03-26"),"capabilities":{"tools":{}},"serverInfo":{"name":"axe-studio","version":"1"}}),
                    ),
                    "ping" => Ok(json!({})),
                    "tools/list" => Ok(
                        json!({"tools":tool_specs().iter().map(|s|json!({"name":s["name"],"description":s["description"],"inputSchema":s["inputSchema"]})).collect::<Vec<_>>()}),
                    ),
                    "tools/call" => {
                        let p = &v["params"];
                        let call = HostCall {
                            name: p["name"].as_str().unwrap_or("").into(),
                            arguments: p["arguments"].clone(),
                        };
                        let (rtx, rrx) = mpsc::sync_channel::<ToolReply>(1);
                        if tx.try_send((call, rtx)).is_err() {
                            Err("Tool queue busy".into())
                        } else {
                            match rrx.recv_timeout(Duration::from_secs(30)) {
                                Ok(result) => Ok(tool_result_content(result)),
                                Err(_) => Err("Tool request timed out".into()),
                            }
                        }
                    }
                    _ => Err("Unknown app method".into()),
                };
                let response = match result {
                    Ok(result) => json!({"jsonrpc":"2.0","id":id,"result":result}),
                    Err(message) => {
                        json!({"jsonrpc":"2.0","id":id,"error":{"code":-32601,"message":message}})
                    }
                };
                let response = tiny_http::Response::from_string(response.to_string()).with_header(
                    tiny_http::Header::from_bytes("Content-Type", "application/json").unwrap(),
                );
                let _ = request.respond(response);
            }
        });
        Ok(Self {
            url,
            token,
            stop,
            calls,
        })
    }
}
impl Drop for Mcp {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::SeqCst);
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn tool_audio_result_is_sent_as_audio_content_not_text_data() {
        let result = tool_result_content(Ok(json!({
            "sample":{"name":"riff.wav"},
            "audio_attachment":{"mime_type":"audio/wav","data_base64":"UklGRAA="}
        })));
        assert_eq!(result["isError"], false);
        assert_eq!(result["content"][1]["type"], "audio");
        assert_eq!(result["content"][1]["mimeType"], "audio/wav");
        assert!(
            !result["content"][0]["text"]
                .as_str()
                .unwrap()
                .contains("UklGRAA=")
        );
    }
}
