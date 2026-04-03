# Introduction

`monochange` is a cross-ecosystem release planner for monorepos.

The current milestone focuses on:

<!-- {=projectMilestoneCapabilities} -->

- discover Cargo, npm/pnpm/Bun, Deno, Dart, and Flutter packages
- normalize dependency edges across ecosystems
- coordinate shared package groups from `monochange.toml`
- compute release plans from explicit change input
- expose top-level CLI commands from `[cli.<command>]` definitions
- run config-defined release commands from `.changeset/*.md`
- render changelogs through structured release notes and configurable formats
- emit stable release-manifest JSON for downstream automation
- preview or publish provider releases and release requests from typed command steps and shared release data
- enforce pull-request changeset policy through typed command steps and reusable diagnostics
- apply Rust semver evidence when provided
- expose built-in assistant setup guidance with `mc assist` and a stdio MCP server with `mc mcp`
- publish the CLI as `@monochange/cli` and the bundled agent skill as `@monochange/skill`
- publish end-user documentation through the mdBook in `docs/`

<!-- {/projectMilestoneCapabilities} -->

## GitHub automation

<!-- {=projectGitHubAutomationOverview} -->

MonoChange can promote one prepared release into several source-provider automation flows without changing the underlying release-plan model.

- `mc release-manifest` writes a stable JSON artifact for downstream jobs, including authored changesets plus linked release context metadata
- `mc publish-release --dry-run --format json` previews provider release payloads before publishing
- `mc release-pr --dry-run --format json` previews the release branch, commit, and release-request body
- changelog templates can render linked change owners, review requests, commits, and closed issues through `{{ context }}` or fine-grained metadata variables
- `mc verify --format json --changed-paths ...` evaluates pull-request changeset policy from CI-supplied paths and labels

<!-- {/projectGitHubAutomationOverview} -->

## Core workflow

<!-- {=projectCoreWorkflow} -->

Initialize the repository with detected packages, groups, and default CLI commands:

```bash
mc init
```

The generated `monochange.toml` becomes the source of truth for top-level commands like `mc validate`, `mc discover`, `mc change`, and `mc release`.

Validate the repository:

```bash
mc validate
```

Discover the workspace:

```bash
mc discover --format json
```

Create a change file:

```bash
mc change --package monochange --bump minor --reason "add release planning"
```

Preview the release command:

```bash
mc release --dry-run --format json
```

Prepare the release:

```bash
mc release
```

<!-- {/projectCoreWorkflow} -->

## Assistant setup and MCP

Install the prebuilt CLI with npm:

```bash
npm install -g @monochange/cli
monochange --help
mc --help
```

Install the bundled skill package when you want reusable agent guidance:

```bash
npm install -g @monochange/skill
monochange-skill --print-install
monochange-skill --copy ~/.pi/agent/skills/monochange
```

Print an assistant profile with install instructions, repo-local guidance, and MCP configuration:

```bash
mc assist pi
```

Start the MCP server over stdin/stdout:

```bash
mc mcp
```

Typical MCP client config:

```json
{
	"mcpServers": {
		"monochange": {
			"command": "monochange",
			"args": ["mcp"]
		}
	}
}
```

Run the full validation suite:

<!-- {=projectValidationCommands} -->

```bash
docs:verify
lint:all
test:all
build:all
build:book
```

<!-- {/projectValidationCommands} -->

## What the JSON output includes

Discovery output includes:

<!-- {=projectDiscoveryOutputIncludes} -->

- normalized package records
- dependency edges
- release groups derived from configured groups
- warnings

<!-- {/projectDiscoveryOutputIncludes} -->

Release-plan output includes:

<!-- {=projectReleaseOutputIncludes} -->

- per-package bump decisions
- synchronized group outcomes
- compatibility evidence
- warnings and unresolved items

<!-- {/projectReleaseOutputIncludes} -->
