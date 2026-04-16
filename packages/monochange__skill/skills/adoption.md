# Adoption skill

Use this guide when the user wants help deciding **how** to adopt monochange, not just which command to run next.

## First move: inspect before interrogating

Inspect the repository before asking detailed setup questions.

Look for:

- package ecosystems and workspace managers
- existing CI files such as `.github/workflows/*` or `.gitlab-ci.yml`
- existing release tooling such as changesets, knope, semantic-release, release-please, or custom scripts
- whether packages appear public, private, or mixed
- existing changelog, tag, and release-branch conventions

Use that evidence to decide confidence:

- **high confidence** — recommend a default and ask for confirmation
- **medium confidence** — recommend a default and ask one or two targeted questions
- **low confidence** — ask more questions before proposing file changes

## Ask setup depth first

Start with one question:

- `quickstart` — generate or refine config and stop at validation plus dry-run release planning
- `standard` — config, linting, and release preview
- `full` — config, linting, CI automation, publishing, and placeholder strategy when relevant
- `migration` — phase monochange into an existing repository safely

## Core question tree

### 1. Repository starting point

Ask:

- is this a greenfield repository or an existing one?
- if it already exists, what handles releases today?
- what must stay compatible during the first adoption phase?

Recommendation:

- prefer coexistence first for existing repositories
- replace old tooling only after monochange discovery and dry-run planning are trusted

### 2. Workspace shape

Ask:

- which ecosystems are present?
- which packages are public, private, or internal-only?
- should versions stay package-specific or move into groups?

Recommendation:

- prefer package ids first
- create groups only when the outward release identity is truly shared

### 3. Linting depth

Ask:

- does the team want minimal guardrails or stronger manifest consistency?

Recommendation:

- `minimal` — start with a small `[lints.rules]` set for publication-safety rules only
- `balanced` — prefer `[lints].use = ["cargo/recommended", "npm/recommended"]` plus a few targeted overrides
- `strict` — promote stricter presets or scoped overrides after the repo is already stable

### 4. Release orchestration

Ask:

- should releases stay local-only, use a merged release commit, or use a long-running release PR branch?
- which provider owns CI: GitHub, GitLab, or something custom?

Recommendation:

- default to hybrid: local discovery and dry-runs, CI for real release and publish work
- prefer GitHub for the most automated builtin path today
- on GitLab, keep planning builtin and keep publishing external more often when auth/bootstrap is already specialized

### 5. Publishing and placeholders

Ask these only if the workspace contains public packages.

Ask:

- which registries are involved?
- does the team want builtin or external publish jobs?
- do package names need to be reserved before the real release flow is ready?

Recommendation:

- GitHub + npm: builtin is the preferred default
- `crates.io` and `pub.dev`: external is often clearer when the registry-maintained workflow should own the publish step
- ask about `mc placeholder-publish` only when public names matter and the first real release may be delayed

## Final output contract

End the planning flow with a compact decision record:

- detected repo profile
- chosen setup depth
- recommended adoption path
- recommended `monochange.toml` shape
- recommended lint profile
- recommended CI and release strategy
- recommended publishing and placeholder strategy, if applicable
- open risks or unanswered questions
- next commands to run

## Guardrails

- do not write CI or publish configuration without permission
- do not force publishing questions into internal-only repositories
- do not recommend big-bang migration by default
- keep recommendations short, with the default plus a brief tradeoff explanation

## Related examples

- [../examples/README.md](../examples/README.md)
- full repository examples: <https://github.com/ifiokjr/monochange/tree/main/examples>
