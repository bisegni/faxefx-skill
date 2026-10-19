use crate::{
    SKILL,
    process::Rpc,
    runtime::{HostCall, ToolReply},
};
use serde_json::{Value, json};
use std::{
    path::{Path, PathBuf},
    process::Command,
};
pub const DISABLED: &[&str] = &[
    "shell_tool",
    "unified_exec",
    "code_mode",
    "multi_agent",
    "multi_agent_v2",
    "apps",
    "plugins",
    "hooks",
    "browser_use",
    "computer_use",
    "image_generation",
    "request_permissions_tool",
];
fn tool_reply_content(result: ToolReply) -> Result<(bool, Vec<Value>), String> {
    match result {
        Ok(mut value) => {
            let attachment = value
                .as_object_mut()
                .and_then(|object| object.remove("audio_attachment"));
            let mut items = vec![json!({"type":"inputText","text":value.to_string()})];
            if let Some(audio) = attachment {
                let mime = audio["mime_type"]
                    .as_str()
                    .ok_or("Audio MIME type missing")?;
                let data = audio["data_base64"].as_str().ok_or("Audio data missing")?;
                items.push(
                    json!({"type":"inputAudio","audioUrl":format!("data:{mime};base64,{data}")}),
                );
            }
            Ok((true, items))
        }
        Err(error) => Ok((false, vec![json!({"type":"inputText","text":error})])),
    }
}
pub fn config() -> Value {
    let mut c = serde_json::Map::new();
    for k in DISABLED {
        c.insert(format!("features.{k}"), json!(false));
    }
    c.insert("features.code_mode_host".into(), json!(true));
    c.insert("web_search".into(), json!("disabled"));
    Value::Object(c)
}
pub struct Codex {
    pub rpc: Rpc,
    pub thread: String,
    pub turn: Option<String>,
    skill: PathBuf,
}
impl Codex {
    pub fn connect(exe: &Path, root: &Path, resume: Option<&str>) -> Result<Self, String> {
        let skill = root.join("axefx-live/SKILL.md");
        std::fs::create_dir_all(skill.parent().unwrap()).map_err(|e| e.to_string())?;
        std::fs::write(&skill, SKILL).map_err(|e| e.to_string())?;
        let mut command = Command::new(exe);
        command.arg("app-server").arg("--stdio").current_dir(root);
        for (k, v) in config().as_object().unwrap() {
            command.arg("-c").arg(format!("{k}={v}"));
        }
        let mut rpc = Rpc::spawn(&mut command)?;
        let init=rpc.request("initialize",json!({"clientInfo":{"name":"axe_studio","version":"0.1.0"},"capabilities":{"experimentalApi":true}}))?;
        if !init["userAgent"]
            .as_str()
            .is_some_and(|s| s.contains("0.163."))
        {
            return Err(
                "This build supports Codex 0.163.x. Install a compatible Codex runtime.".into(),
            );
        }
        rpc.send(&json!({"method":"initialized","params":{}}))?;
        let loaded = rpc.request("config/read", json!({"includeLayers":false}))?["config"].clone();
        if DISABLED.iter().any(|k| loaded["features"][*k] != false)
            || loaded["features"]["code_mode_host"] != true
        {
            return Err("Codex did not confirm required tool isolation".into());
        }
        let account = rpc.request("account/read", json!({"refreshToken":false}))?;
        if account["requiresOpenaiAuth"] == true && account["account"].is_null() {
            return Err("Sign in to Codex using its supported login, then retry.".into());
        }
        let mut overrides = config();
        if let Some(servers) = loaded["mcp_servers"].as_object() {
            for name in servers.keys() {
                overrides[format!("mcp_servers.{name}.enabled")] = json!(false);
            }
        }
        // Disable every discovered unrelated skill in this thread without editing user settings.
        let listed = rpc.request("skills/list", json!({"cwds":[root],"forceReload":true}))?;
        let mut skills = vec![];
        if let Some(data) = listed["data"].as_array() {
            for cwd in data {
                if let Some(found) = cwd["skills"].as_array() {
                    for item in found {
                        if let Some(path) = item["path"].as_str() {
                            skills.push(json!({"path":path,"enabled":Path::new(path)==skill}));
                        }
                    }
                }
            }
        }
        overrides["skills.config"] = json!(skills);
        let instructions = format!("{}\n\n{}", crate::runtime::INSTRUCTIONS, SKILL);
        let mut params = json!({"cwd":root,"sandbox":"read-only","approvalPolicy":"never","config":overrides,"environments":[],"dynamicTools":crate::runtime::tool_specs(),"baseInstructions":instructions,"developerInstructions":instructions});
        let method = if let Some(id) = resume {
            params["threadId"] = json!(id);
            params["excludeTurns"] = json!(true);
            "thread/resume"
        } else {
            "thread/start"
        };
        let response = rpc.request(method, params)?;
        if response["sandbox"]["type"] != "readOnly" {
            return Err("Codex sandbox was not confirmed read-only".into());
        }
        let thread = response["thread"]["id"]
            .as_str()
            .ok_or("Codex thread identity missing")?
            .to_owned();
        Ok(Self {
            rpc,
            thread,
            turn: None,
            skill,
        })
    }
    pub fn start(&mut self, text: &str, init: bool) -> Result<(), String> {
        let mut input = vec![json!({"type":"text","text":text})];
        if init {
            input.push(json!({"type":"skill","name":"axefx-live","path":self.skill}));
        }
        let r = self
            .rpc
            .request("turn/start", json!({"threadId":self.thread,"input":input}))?;
        self.turn = Some(
            r["turn"]["id"]
                .as_str()
                .ok_or("Missing provider turn")?
                .into(),
        );
        Ok(())
    }
    pub fn interrupt(&mut self) -> Result<(), String> {
        if let Some(t) = &self.turn {
            self.rpc
                .request("turn/interrupt", json!({"threadId":self.thread,"turnId":t}))?;
        }
        Ok(())
    }
    pub fn reply(&self, request: &Value, result: ToolReply) -> Result<(), String> {
        let (success, content_items) = tool_reply_content(result)?;
        self.rpc.send(
            &json!({"id":request["id"],"result":{"success":success,"contentItems":content_items}}),
        )
    }
    pub fn host_call(&self, v: &Value) -> Result<HostCall, String> {
        let p = &v["params"];
        if v["method"] != "item/tool/call"
            || p["threadId"] != self.thread
            || p["turnId"].as_str() != self.turn.as_deref()
        {
            return Err("Tool call does not belong to active turn".into());
        }
        Ok(HostCall {
            name: p["tool"].as_str().unwrap_or("").into(),
            arguments: p["arguments"].clone(),
        })
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn explicit_audio_tool_output_becomes_native_input_audio() {
        let (_, items) = tool_reply_content(Ok(json!({
            "sample":{"id":"sample-id","name":"riff.wav"},
            "audio_attachment":{"mime_type":"audio/wav","data_base64":"UklGRAA="}
        })))
        .unwrap();
        assert_eq!(items[1]["type"], "inputAudio");
        assert_eq!(items[1]["audioUrl"], "data:audio/wav;base64,UklGRAA=");
        assert!(!items[0].to_string().contains("audio_attachment"));
    }
}
