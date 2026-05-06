# monochange_app — Architecture & Implementation Plan

**Status**: Decided (grilling complete)
**Created**: 2026-05-01
**Branch**: `feat/monochange-app-planning`
**Domain**: `monochange.dev`

---

## Decisions (from grilling session)

| # | Decision | Choice |
|---|----------|--------|
| 1 | MVP slice | GitHub App → automated changesets + release PRs |
| 2 | Workspace layout | Separate `apps/monochange_app` workspace |
| 3 | Database | PostgreSQL with Welds ORM |
| 4 | Deployment | Fly.io |
| 5 | LLM | OpenRouter API first, Ollama for local dev |
| 6 | Pricing | Per-repo + seat limits + AI quotas |
| 7 | Timeline | Build alongside CLI |
| 8 | CLI relationship | CLI stays standalone open-source; SaaS is separate value-add |
| 9 | AI scoping quality | Directional enough to start a conversation |
| 10 | GitLab | Future work (not MVP) |

---

## Phase 0: Foundation (Weeks 1-2)

### 0.1 — Scaffold the workspace

```
apps/monochange_app/
├── Cargo.toml              # [workspace] with members = ["crates/*"]
├── Cargo.lock
├── rust-toolchain.toml     # Pin stable Rust
├── crates/
│   ├── monochange_app/          # Leptos SPA (WASM + SSR server)
│   │   ├── Cargo.toml
│   │   ├── src/
│   │   │   ├── lib.rs           # WASM client entrypoint, Leptos app
│   │   │   ├── main.rs          # Server binary (axum + Leptos SSR)
│   │   │   ├── app.rs           # <App/> component tree
│   │   │   ├── error.rs         # AppError type, error rendering
│   │   │   ├── routes/          # Leptos route components
│   │   │   ├── pages/           # Page-level components
│   │   │   └── components/      # Shared UI components
│   │   └── style/
│   │       └── main.css
│   ├── monochange_app_db/       # Database models & migrations (Welds)
│   │   ├── Cargo.toml
│   │   ├── src/
│   │   │   ├── lib.rs
│   │   │   ├── models/          # WeldsModel structs
│   │   │   │   ├── mod.rs
│   │   │   │   ├── user.rs
│   │   │   │   ├── organization.rs
│   │   │   │   ├── installation.rs
│   │   │   │   ├── repository.rs
│   │   │   │   ├── changeset.rs
│   │   │   │   ├── feedback_form.rs
│   │   │   │   ├── feedback_submission.rs
│   │   │   │   ├── roadmap_item.rs
│   │   │   │   └── ai_request.rs
│   │   │   └── migrations/
│   │   │       └── mod.rs       # Welds migration definitions
│   │   └── templates/           # Minijinja templates for changelogs
│   ├── monochange_app_api/      # API logic, auth, webhooks
│   │   ├── Cargo.toml
│   │   └── src/
│   │       ├── lib.rs
│   │       ├── auth.rs          # GitHub OAuth, session management
│   │       ├── webhooks.rs      # GitHub webhook receiver
│   │       ├── api/             # REST API handlers
│   │       │   ├── mod.rs
│   │       │   ├── repos.rs
│   │       │   ├── changesets.rs
│   │       │   ├── feedback.rs
│   │       │   ├── roadmap.rs
│   │       │   └── ai.rs
│   │       └── middleware.rs    # Auth middleware, rate limiting
│   └── monochange_app_ai/       # AI agents (OpenRouter client)
│       ├── Cargo.toml
│       └── src/
│           ├── lib.rs
│           ├── client.rs        # OpenRouter HTTP client
│           ├── scoping.rs       # Feature scoping agent
│           ├── changeset.rs     # Issue-to-changeset agent
│           └── changelog.rs     # Changelog polishing agent
├── embed/                       # JS feedback widget
│   ├── package.json
│   ├── src/
│   │   ├── index.ts             # Widget entrypoint
│   │   ├── api.ts               # monochange API client (fetch)
│   │   └── styles.css
│   └── build.js                 # esbuild/vite build config
├── fly.toml                     # Fly.io deployment config
├── Dockerfile                   # Multi-stage Rust build
└── README.md
```

**Dependencies** (from parent workspace via path):
- `monochange_core` — ReleaseManifest, ChangesetFile, ProviderReleaseNotesSource
- `monochange_github` — GitHub App installation tokens, release creation
- `monochange_hosting` — release_body, release_pull_request_body

### 0.2 — Database schema (Welds models)

```rust
// crates/monochange_app_db/src/models/user.rs
#[derive(Debug, WeldsModel)]
#[welds(schema = "public", table = "users")]
#[welds(HasMany(installations, super::installation::Installation, "user_id"))]
#[welds(HasMany(organizations, super::organization::OrganizationMember, "user_id"))]
pub struct User {
    #[welds(primary_key)]
    pub id: i32,
    pub github_id: i64,            // GitHub's user ID
    pub github_login: String,
    pub github_avatar_url: Option<String>,
    pub github_access_token: String, // encrypted at rest
    pub email: Option<String>,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub updated_at: chrono::DateTime<chrono::Utc>,
    pub plan_tier: String,         // "free", "pro", "team", "enterprise"
}

#[derive(Debug, WeldsModel)]
#[welds(schema = "public", table = "organizations")]
pub struct Organization {
    #[welds(primary_key)]
    pub id: i32,
    pub github_id: i64,
    pub github_login: String,
    pub github_avatar_url: Option<String>,
}

#[derive(Debug, WeldsModel)]
#[welds(schema = "public", table = "organization_members")]
#[welds(BelongsTo(user, super::user::User, "user_id"))]
#[welds(BelongsTo(org, super::organization::Organization, "org_id"))]
pub struct OrganizationMember {
    #[welds(primary_key)]
    pub id: i32,
    pub user_id: i32,
    pub org_id: i32,
    pub role: String,              // "admin", "member"
}

#[derive(Debug, WeldsModel)]
#[welds(schema = "public", table = "installations")]
#[welds(BelongsTo(user, super::user::User, "user_id"))]
#[welds(HasMany(repos, super::repository::Repository, "installation_id"))]
pub struct Installation {
    #[welds(primary_key)]
    pub id: i32,
    pub user_id: i32,
    pub github_installation_id: i64,
    pub github_account_login: String, // the org or user that installed
    pub github_account_type: String,  // "User" or "Organization"
    pub target_type: String,          // "selected" or "all"
    pub created_at: chrono::DateTime<chrono::Utc>,
}

#[derive(Debug, WeldsModel)]
#[welds(schema = "public", table = "repositories")]
#[welds(BelongsTo(installation, super::installation::Installation, "installation_id"))]
#[welds(HasMany(changesets, super::changeset::PendingChangeset, "repository_id"))]
pub struct Repository {
    #[welds(primary_key)]
    pub id: i32,
    pub installation_id: i32,
    pub github_repo_id: i64,
    pub github_full_name: String,    // "owner/repo"
    pub github_private: bool,
    pub monochange_config_hash: Option<String>,
    pub settings_json: Option<String>, // JSON blob for repo-specific settings
    pub plan_tier: String,           // overrides org/user tier if higher
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub updated_at: chrono::DateTime<chrono::Utc>,
}

#[derive(Debug, WeldsModel)]
#[welds(schema = "public", table = "pending_changesets")]
#[welds(BelongsTo(repository, super::repository::Repository, "repository_id"))]
pub struct PendingChangeset {
    #[welds(primary_key)]
    pub id: i32,
    pub repository_id: i32,
    pub filename: String,            // e.g. "2026-05-01-awesome-feature.md"
    pub source_github_issue: Option<i64>,
    pub bump: String,                // "major", "minor", "patch", "none"
    pub package_id: String,
    pub summary: String,
    pub ai_generated: bool,
    pub pr_number: Option<i64>,      // the PR that created this changeset
    pub status: String,              // "pending", "released", "reverted"
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub released_at: Option<chrono::DateTime<chrono::Utc>>,
}

#[derive(Debug, WeldsModel)]
#[welds(schema = "public", table = "feedback_forms")]
#[welds(BelongsTo(repository, super::repository::Repository, "repository_id"))]
pub struct FeedbackForm {
    #[welds(primary_key)]
    pub id: i32,
    pub repository_id: i32,
    pub name: String,
    pub slug: String,                // URL-safe identifier
    pub allowed_domains: Option<String>, // JSON array of origins for CORS
    pub custom_css: Option<String>,
    pub enabled: bool,
    pub created_at: chrono::DateTime<chrono::Utc>,
}
```

### 0.3 — Core infrastructure tasks

- [ ] Create `apps/monochange_app/Cargo.toml` workspace
- [ ] Wire `monochange_core`, `monochange_github`, `monochange_hosting` as path deps
- [ ] Set up `sqlx` + `welds` with PostgreSQL connection pool
- [ ] Create initial Welds migrations (users, organizations, installations, repositories)
- [ ] Implement GitHub OAuth flow (cookie-based sessions or JWT)
- [ ] Set up `axum` server with Leptos SSR integration
- [ ] Deploy placeholder to Fly.io with PostgreSQL
- [ ] Set up GitHub App registration (manifest flow)
- [ ] Implement webhook verification (HMAC signature check)

---

## Phase 1: GitHub App + Automated Actions (Weeks 3-6)

### 1.1 — GitHub App webhook flow

```
GitHub Event → POST /api/webhooks/github → verify HMAC → route by event type
                                              │
   ┌──────────────────────────────────┬────────┴────────┐
   ▼                                  ▼                 ▼
installation.created           issues.opened        pull_request.*
→ store Installation           → AI: scope it      → check for changesets
→ store repos                  → create changeset  → trigger release PR
```

### 1.2 — Automated changeset creation

When a user opens an issue labeled `feature` or `bug`:

1. Webhook receives `issues.opened`
2. Queue AI processing job:
   - Read issue body + title
   - Read repository's `monochange.toml` (via GitHub API, cached)
   - Prompt OpenRouter with structured output schema
   - AI returns: package_id, bump severity, summary
3. Create `.changeset/*.md` file via GitHub API (commit to a branch + open PR)
   - Commit author: `monochange[bot]`
   - PR title: `changeset: <summary>` 
   - PR body: explains what was done, links issue
4. Store in `pending_changesets` table

### 1.3 — Release PR management

When `mc release` runs (either via CLI or triggered by PR merge):

1. The existing monochange release flow computes the release plan
2. If the repo has the GitHub App installed, instead of doing it locally:
   - GitHub App opens/updates a "Release PR" branch
   - Updates versions, changelogs, deletes consumed changesets
   - Opens PR with rendered release notes

### 1.4 — Phase 1 deliverables

- [ ] GitHub OAuth sign-in working on `monochange.dev`
- [ ] GitHub App manifest / installation flow
- [ ] Webhook receiver with event routing
- [ ] Issue → changeset automation (AI-powered)
- [ ] Release PR automation
- [ ] Dashboard page showing connected repos, recent activity
- [ ] Settings page (per-repo configuration)

---

## Phase 2: Dashboard & Roadmap (Weeks 7-10)

### 2.1 — Dashboard pages

```
/dashboard                    → Overview: connected repos, PRs, changesets
/settings                     → Account, billing, GitHub App management
/:owner/:repo                 → Repo detail
/:owner/:repo/changelog       → Public changelog (beautiful, user-facing)
/:owner/:repo/roadmap         → Public roadmap (AI-curated)
/:owner/:repo/feedback        → Feedback form management
```

### 2.2 — Public roadmap feature

Data model:
```rust
#[derive(Debug, WeldsModel)]
#[welds(schema = "public", table = "roadmap_items")]
pub struct RoadmapItem {
    #[welds(primary_key)]
    pub id: i32,
    pub repository_id: i32,
    pub title: String,
    pub description: String,
    pub status: String,            // "planned", "in_progress", "shipped", "rejected"
    pub source_issue: Option<i64>,
    pub source_feedback: Option<i32>, // linked feedback submission
    pub votes: i32,                // aggregated from feedback
    pub position: i32,             // sort order
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub updated_at: chrono::DateTime<chrono::Utc>,
}
```

The roadmap page:
- Shows items grouped by status (kanban-ish or simple list)
- Users can vote on roadmap items (stored in feedback submissions / reactions)
- AI generates status updates as linked PRs are merged

### 2.3 — Changelog page

Uses the existing `monochange_hosting::release_body` and `monochange_hosting::release_pull_request_body` to render release notes. The page is:
- Public (no auth needed for public repos)
- Beautifully styled (leptos SSR with good typography)
- Filterable by package/group, version range
- Has an RSS/Atom feed for each repo
- Supports custom domains (CNAME) for enterprise users

---

## Phase 3: Feedback Forms (Weeks 11-14)

### 3.1 — Embed widget

The feedback widget lives at `embed/` — a small TypeScript bundle (~20KB gzipped max):

```typescript
// embed/src/index.ts
class MonochangeWidget {
  constructor(config: { formId: string; repo: string; theme?: 'light' | 'dark' }) { ... }
  mount(container: HTMLElement): void { ... }
  show(): void { ... }
  hide(): void { ... }
}

// Auto-init from script tag
(window as any).MonochangeWidget = MonochangeWidget;
```

API endpoints:
```
POST /api/feedback/:repo_slug/:form_slug/submit  → store submission
GET  /api/feedback/:repo_slug/:form_slug/config   → fetch form config (fields, styling)
```

### 3.2 — Terminal feedback

A CLI command (or simple curl one-liner):
```bash
curl -X POST https://monochange.dev/api/feedback/owner/repo/main/submit \
  -H "Content-Type: application/json" \
  -d '{"feedback": "Would love dark mode!", "email": "user@example.com"}'
```

Or: `mc feedback` in the CLI that submits to the monochange API.

### 3.3 — Feedback → AI triage

When feedback is submitted:
1. AI categorizes (bug report, feature request, praise, question)
2. If feature request, AI generates a draft roadmap item
3. Maintainer can approve/reject with one click
4. Similar feedback is grouped/deduped

---

## Phase 4: Monetization & Scale (Weeks 15+)

### 4.1 — Billing integration

- Stripe Checkout for subscriptions
- Plans: Free / Pro ($29/mo) / Team ($99/mo) / Enterprise (custom)
- Metered billing for AI requests beyond quota
- Per-repo tier overrides

### 4.2 — Ollama self-hosting support

Once the API-based approach is stable:
- Add optional Ollama endpoint configuration per-org
- Fall back to OpenRouter if Ollama is unavailable
- Privacy mode: never send code/issue content to external APIs

### 4.3 — GitLab support

- `monochange_gitlab` crate already exists
- Add GitLab OAuth + webhook receiver
- GitLab doesn't have "GitHub Apps" but has OAuth Apps + Project Access Tokens + System Hooks
- Model: org-level OAuth app + webhook configuration per project

---

## Architecture diagram

```
                         monochange.dev (Fly.io)
┌────────────────────────────────────────────────────────────────┐
│                                                                │
│  ┌──────────────────────┐    ┌──────────────────────────────┐ │
│  │   Leptos SSR (axum) │    │      Background Workers       │ │
│  │                      │    │                              │ │
│  │  • Serve WASM SPA    │    │  • AI processing jobs        │ │
│  │  • GitHub webhooks   │    │  • Changeset generation      │ │
│  │  • REST API          │    │  • Feedback triage           │ │
│  │  • OAuth             │    │  • Release PR management     │ │
│  │  • SSR pages         │    │  • Changelog rendering       │ │
│  └──────────┬───────────┘    └──────────────┬───────────────┘ │
│             │                               │                  │
│             └───────────┬───────────────────┘                  │
│                         ▼                                      │
│               ┌──────────────────┐                             │
│               │    PostgreSQL     │                             │
│               │  (Fly Postgres)   │                             │
│               └──────────────────┘                             │
│                         │                                      │
└─────────────────────────┼──────────────────────────────────────┘
                          │
   ┌──────────────────────┼──────────────────────┐
   ▼                      ▼                      ▼
┌──────────┐     ┌──────────────┐      ┌──────────────┐
│  GitHub   │     │  OpenRouter   │      │   Stripe      │
│  API      │     │  (LLM API)    │      │  (Billing)    │
└──────────┘     └──────────────┘      └──────────────┘
```

---

## Key dependency versions

| Crate | Version | Purpose |
|-------|---------|---------|
| `leptos` | 0.7.x | WASM SPA framework |
| `leptos_axum` | 0.7.x | SSR + axum integration |
| `axum` | 0.8.x | HTTP server |
| `welds` | 0.4.x | async ORM (PostgreSQL) |
| `sqlx` | 0.8.x | SQL driver (used under welds) |
| `reqwest` | 0.12.x | HTTP client (OpenRouter) |
| `octocrab` | 0.49.x | GitHub API (already in workspace) |
| `minijinja` | 2.x | Template rendering (already in workspace) |
| `tokio` | 1.x | async runtime |
| `serde` / `serde_json` | 1.x | serialization |
| `chrono` | 0.4.x | timestamps |
| `tower` / `tower-http` | 0.5.x / 0.6.x | middleware, CORS, rate limiting |

---

## OpenRouter model selection strategy

For different AI tasks, route to different models based on cost/capability:

| Task | Model | Approx cost per request |
|------|-------|------------------------|
| Issue → changeset | `anthropic/claude-3-haiku` | ~$0.001 |
| Feature scoping | `openai/gpt-4o-mini` | ~$0.003 |
| Changelog polishing | `anthropic/claude-3-haiku` | ~$0.001 |
| Feedback categorization | `mistralai/mistral-nemo` | ~$0.0005 |
| Triage dedup | `meta-llama/llama-3.1-8b-instruct` | ~$0.0002 |

Structure all AI prompts with `response_format: { type: "json_schema", json_schema: {...} }` for deterministic outputs.

---

## Next steps (immediate)

1. **Register the GitHub App** — create a placeholder app on GitHub, get App ID + private key
2. **Buy `monochange.dev`** — check availability, register
3. **Scaffold `apps/monochange_app/`** — empty workspace, get cargo build passing
4. **Wire up Fly.io** — `fly launch`, provision Postgres, get deployment working with a "hello world" Leptos page
5. **Implement OAuth flow** — sign in with GitHub, create user record
6. **Implement webhook receiver** — verify HMAC, log events
7. **Ship Phase 0 complete** — a user can sign in, connect repos, see their repo list

The first user-facing demo should be: *"Sign in with GitHub → install the app → open an issue with a feature label → monochange automatically creates a changeset PR"*
