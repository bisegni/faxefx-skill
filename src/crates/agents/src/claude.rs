use crate::{SKILL, mcp::Mcp, process::Rpc};
use serde_json::json;
use std::{
    path::{Path, PathBuf},
    process::Command,
};
pub struct Claude {
    exe: PathBuf,
    root: PathBuf,
    pub session: Option<String>,
    pub rpc: Option<Rpc>,
    pub mcp: Mcp,
    key: Option<String>,
    config: PathBuf,
}
impl Claude {
    pub fn connect(
        exe: &Path,
        root: &Path,
        resume: Option<&str>,
        key: Option<String>,
    ) -> Result<Self, String> {
        let version = Command::new(exe)
            .arg("--version")
            .output()
            .map_err(|e| e.to_string())?;
        let version = String::from_utf8_lossy(&version.stdout);
        let n = version.split_whitespace().next().unwrap_or("");
        let parts = n.split('.').collect::<Vec<_>>();
        if parts.len() != 3
            || parts[0] != "2"
            || parts[1] != "1"
            || parts[2].parse::<u32>().unwrap_or(0) < 286
        {
            return Err("Claude Code 2.1.286 or newer compatible 2.1.x is required".into());
        }
        if key.as_ref().is_none_or(|k| k.trim().is_empty())
            && std::env::var_os("ANTHROPIC_API_KEY").is_none()
        {
            return Err("Claude requires supported API authentication. Set ANTHROPIC_API_KEY before launching Axe Studio; subscription login is not used.".into());
        }
        let mcp = Mcp::start()?;
        let config = root.join(format!("mcp-{}.json", uuid::Uuid::new_v4()));
        let mut options = std::fs::OpenOptions::new();
        options.write(true).create_new(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        {
            use std::io::Write;
            let mut file = options.open(&config).map_err(|e| e.to_string())?;
            file.write_all(json!({"mcpServers":{"axe_studio":{"type":"http","url":mcp.url,"headers":{"Authorization":format!("Bearer {}",mcp.token)}}}}).to_string().as_bytes()).map_err(|e|e.to_string())?;
        }
        Ok(Self {
            exe: exe.into(),
            root: root.into(),
            session: resume.map(str::to_owned),
            rpc: None,
            mcp,
            key,
            config,
        })
    }
    pub fn start(&mut self, text: &str) -> Result<(), String> {
        // New process each turn. Bare mode removes unrelated customizations; no built-ins.
        let mut c = Command::new(&self.exe);
        if let Some(key) = &self.key {
            c.env("ANTHROPIC_API_KEY", key);
        }
        c.env_remove("CLAUDE_CODE_OAUTH_TOKEN");
        c.current_dir(&self.root).args([
            "--bare",
            "--restricted",
            "--print",
            "--verbose",
            "--output-format",
            "stream-json",
            "--include-partial-messages",
            "--tools",
            "",
            "--strict-mcp-config",
            "--setting-sources",
            "",
            "--permission-mode",
            "dontAsk",
            "--permission-prompts",
            "none",
            "--no-chrome",
            "--max-turns",
            "12",
        ]);
        c.arg("--system-prompt").arg(format!(
            "{}\n\nExplicitly loaded axefx-live version 1:\n{SKILL}",
            crate::runtime::INSTRUCTIONS
        ));
        c.arg("--mcp-config").arg(&self.config);
        c.arg("--allowedTools").arg(
            crate::TOOLS
                .iter()
                .map(|(n, _)| format!("mcp__axe_studio__{n}"))
                .collect::<Vec<_>>()
                .join(","),
        );
        if let Some(id) = &self.session {
            c.arg("--resume").arg(id);
        }
        let rpc = Rpc::spawn(&mut c)?;
        // Prompt goes through stdin, never command-line process listings.
        rpc.write_text(text)?;
        self.rpc = Some(rpc);
        Ok(())
    }
    pub fn interrupt(&mut self) {
        self.rpc = None;
    }
}

impl Drop for Claude {
    fn drop(&mut self) {
        self.rpc = None;
        let _ = std::fs::remove_file(&self.config);
    }
}
