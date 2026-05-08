# monochange_app

Release planning toolkit for monorepos — web application.

## Development

Run app commands from the repository root so the shared devenv shell and process manager are available.

```bash
# Start PostgreSQL for the app database.
devenv up -d postgres

# Start the Leptos SSR dev server.
devenv shell cargo leptos --manifest-path apps/monochange_app/crates/monochange_app/Cargo.toml serve

# Refresh the checked-in Tailwind CSS bundle after editing crates/monochange_app/style/input.css.
devenv shell -- bash -lc 'cd apps/monochange_app && tailwindcss --input crates/monochange_app/style/input.css --output crates/monochange_app/style/output.css'

# Run Playwright CLI smoke tests and screenshot capture.
# Artifacts default to $TMPDIR/monochange-app-playwright, outside the git worktree.
pnpm --filter @monochange/app test:screenshots

# Run the mockable release automation scheduler tests.
devenv shell cargo test --manifest-path apps/monochange_app/Cargo.toml --package monochange_app_automation

# Run the PostgreSQL-backed scheduler store test in an isolated temporary schema.
devenv up -d postgres
MONOCHANGE_APP_RUN_DB_TESTS=1 devenv shell cargo test --manifest-path apps/monochange_app/Cargo.toml --package monochange_app_automation postgres_store_runs_durable_job_lifecycle

# Start the app with the dry-run automation worker enabled.
# This processes due release_schedules rows without GitHub credentials or repository writes.
MONOCHANGE_APP_AUTOMATION=dry-run devenv shell cargo leptos --manifest-path apps/monochange_app/crates/monochange_app/Cargo.toml serve
```

## Architecture

```
apps/monochange_app/
├── crates/
│   ├── monochange_app/      # Leptos SPA + axum SSR server
│   ├── monochange_app_db/   # Welds ORM models + migrations
│   ├── monochange_app_api/  # OAuth, webhooks, REST handlers
│   ├── monochange_app_ai/   # OpenRouter client + AI agents
│   └── monochange_app_automation/ # GitHub App permissions + mockable release scheduler
└── embed/                   # JS feedback widget
```

## GitHub App automation model

The app uses OAuth for user identity, but repository automation should run through GitHub App installation tokens. The automation crate models the permissions and release cadence without touching GitHub, git, or Postgres so tests can simulate repo states entirely in memory.

Default hosted release automation capabilities require:

- `Metadata: read`
- `Contents: write` for release branches, commits, tags, and GitHub Releases
- `Pull requests: write` for release PRs
- `Actions: write` for `workflow_dispatch` / `repository_dispatch`
- `Checks: read` and `Commit statuses: write` for release status reporting
- `Issues: write` for released-issue comments
- `Deployments: write` when a cadence is represented as staged deployments

`Workflows: write` is intentionally separate and only needed if monochange edits `.github/workflows/*.yml`.

The scheduler supports interval cadences and staged windows such as “four release batches every four hours, then wait 24 hours before the next window.” Tests use `InMemoryReleaseJobStore`, `FakeReleasePlanner`, `FakeGitHubAutomationClient`, and `FixedClock` so release behavior can be verified without a real repository or GitHub App. The production-facing `PostgresReleaseJobStore` uses the same trait and has an opt-in integration test that creates and drops an isolated schema.

The SSR server does not start release automation by default. Set `MONOCHANGE_APP_AUTOMATION=dry-run` to start the background worker with `DryRunReleasePlanner` and `DryRunGitHubAutomationClient`; it exercises the durable queue while intentionally avoiding GitHub network calls and repository writes.
