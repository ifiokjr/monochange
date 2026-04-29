---
monochange: minor
monochange_core: minor
---

# Expose resolved configuration in CLI templates

Configured CLI workflows can now reference the resolved workspace configuration and paths without adding a dedicated setup step. Templates receive `config`, `project_root`, and `config_path` by default, so later steps can read values such as `{{ config.packages.0.id }}` while regular workflow execution stays quiet unless a step intentionally emits output.

The new built-in config step also makes the generated command available for CI and debugging:

```bash
mc step:config
```

It prints JSON containing the canonical project root, the `monochange.toml` path, and the resolved configuration:

```json
{
	"projectRoot": "/workspace/repo",
	"configPath": "/workspace/repo/monochange.toml",
	"config": {
		"packages": []
	}
}
```

This gives scripts and GitHub Actions a stable command for inspecting the exact configuration that `monochange` loaded while preserving the no-output default for configured workflow steps.
