---
monochange: minor
monochange_config: minor
---

#### add assistant setup, MCP server, and npm distribution

Two new built-in commands let AI assistants integrate MonoChange without manual setup:

```bash
# print install instructions and add monochange as an MCP server
mc assist

# start the MCP server so the assistant can call mc commands over JSON-RPC
mc mcp
```

`mc mcp` exposes the core MonoChange operations (`discover`, `change`, `validate`, `release --dry-run`) as MCP tools, so assistants running inside Claude, Cursor, or any MCP-compatible host can invoke them directly without spawning shell commands.

The CLI is also published through npm for teams that prefer installing through their existing package manager:

```bash
npm install -g @monochange/cli
# or install the agent skill for assistant-guided workflows
npm install -g @monochange/skill
```

**`monochange_config`** gains the `[mcp]` and `[assist]` configuration sections that control server transport and install guidance output.
