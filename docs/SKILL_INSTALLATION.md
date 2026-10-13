# Install the Axe-Fx skill

Run setup commands from the repository root. Both clients use the direct Python
file and the same preview/approval workflow.

## Install and use the skill in Codex or Claude Code

Codex and Claude Code share the maintained
[axefx-preset instructions](../plugins/axefx/skills/axefx-preset/SKILL.md) and CLI
reference. Both run this checkout's Python file and require approval of the exact
preview before applying device changes.

| Client | Project skill entrypoint | Invocation |
|---|---|---|
| Codex | `.agents/skills/axefx-preset/SKILL.md` | `$axefx-preset` |
| Claude Code | `.claude/skills/axefx-preset/SKILL.md` | `/axefx-preset` |

### Project-local setup (recommended)

Keep the complete checkout, including its hidden `.agents` and `.claude`
directories. Complete [Python environment setup](CLI.md#set-up-the-python-environment)
first. The skill entrypoints are already included, so no skill copy or application
installation is required when using this workspace.

For **Codex**, open a terminal in the checkout, activate the environment, and run:

```sh
source .venv-cli/bin/activate
codex
```

In the Codex prompt, enter:

```text
$axefx-preset Use the mock backend to inspect my preset and preview a warmer clean tone.
```

For **Claude Code**, open a terminal in the checkout, activate the environment,
and run:

```sh
source .venv-cli/bin/activate
claude
```

In the Claude Code prompt, enter:

```text
/axefx-preset Use the mock backend to inspect my preset and preview a warmer clean tone.
```

On Windows, use the PowerShell Python setup above and tell the agent to use
`.venv-cli\Scripts\python.exe`; the client commands and prompts are the same.
In the Codex desktop client, open this checkout as the workspace and start a new
chat, then invoke `$axefx-preset`. Restart/refresh the client if the skill does not
appear. These discovery locations follow the official
[Codex skill documentation](https://learn.chatgpt.com/docs/build-skills) and
[Claude Code skill documentation](https://code.claude.com/docs/en/skills).

### Optional personal skill installation

To make the skill discoverable outside this workspace, copy the **maintained
skill folder**, including its reference files, to the client's personal skills
directory. Run these macOS/Linux commands from the checkout; choose either client
or both:

```sh
# Codex
mkdir -p "$HOME/.agents/skills"
cp -R -i plugins/axefx/skills/axefx-preset "$HOME/.agents/skills/"

# Claude Code
mkdir -p "$HOME/.claude/skills"
cp -R -i plugins/axefx/skills/axefx-preset "$HOME/.claude/skills/"
```

On Windows, copy the same folder to `%USERPROFILE%\.agents\skills\axefx-preset`
for Codex or `%USERPROFILE%\.claude\skills\axefx-preset` for Claude Code. Restart
the client and use the same invocation shown above. Do not copy only the thin
project entrypoint: its relative workflow link depends on this checkout.

The personal skill still needs this checkout and its Python environment. Start
the client here, or include the absolute checkout and Python interpreter paths
in your request when working elsewhere. Personal copies must be refreshed after
skill updates; if multiple copies appear, select the intended one. The project-
local setup avoids maintaining a second copy.

### Optional plugin setup

The local plugin in `plugins/axefx` has manifests for both clients. For **Codex**,
register this checkout's marketplace and install its `axefx` entry:

```sh
codex plugin marketplace add .
codex plugin add axefx@axefx-local
```

Run these from the checkout. Start a new session and select the Axe-Fx preset
skill from the skill picker. In the desktop client, restart it, open the Plugins
Directory, choose **Axe-Fx III Local**, and install **Axe-Fx III Preset Assistant**.
The marketplace file is `.agents/plugins/marketplace.json`; see the official
[local marketplace instructions](https://developers.openai.com/plugins/build/plugins).

For **Claude Code**, load the plugin for one session from this checkout:

```sh
claude --plugin-dir ./plugins/axefx
```

Then invoke `/axefx:axefx-preset`. This uses Claude Code's
[local plugin loading](https://code.claude.com/docs/en/plugins); no marketplace
installation is needed. The plugin contains instructions only and still needs
this checkout and its Python environment. There is no bundled application, CLI
wheel or MCP server. These entrypoints support local Claude Code sessions; they
do not give browser/cloud Claude access to your computer's USB device.

Command syntax is checked against Codex CLI `0.162.1` and Claude Code `2.1.296`.
Claude Code manifest/skill validation is checked locally. Plugin installation,
interactive discovery and end-to-end client execution remain manual acceptance
checks; the installation commands are documentation examples, not executed here.

