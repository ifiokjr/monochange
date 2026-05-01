# monochange_app — Full Product Vision & Architecture

**Status**: Architecture defined. Development scaffolded.
**Branch**: `feat/monochange-app-planning`
**Domain**: `monochange.dev`

---

## 1. Product thesis

monochange_app is a **community-driven application evolution platform**. It lets developers ship an app, accept feedback from users, and allow AI-managed forks of that app to be deployed so users can live with their changes. Forked changes that gain traction merge back into mainline, making the app evolve through collective intelligence rather than a single roadmap.

**It is both SaaS middleware (other apps integrate it) and self-hosting (monochange manages itself).**

---

## 2. Core concepts

### 2.1 — The Fork

A fork is a **live, deployed variation of an application** that contains one or more changes not yet in the mainline. Forks come in three deployment modes, chosen by the integrating developer:

| Mode | How it works | Best for |
|------|-------------|----------|
| **Feature flag** | Same deployment, logic gated by domain/user/header. No separate infra. | SaaS apps, low-risk changes |
| **Separate deployment** | New subdomain, same codebase, different config. Database isolated or shared. | Medium-risk, UI-heavy changes |
| **Git fork** | Actual `git clone`. Separate repo, separate CI/CD, full isolation. | High-risk, architectural changes |

The AI assists the developer in choosing and configuring the right mode during initial setup.

### 2.2 — The Feedback Queue

End-users submit feedback through embeddable widgets (web, CLI, SDK). Each submission:

1. Is stored and categorized by AI (bug, feature, praise, question)
2. Is deduplicated against existing feedback
3. Generates a draft "proposal" — a scoped description of what the change would be
4. Appears in the developer's queue for triage

### 2.3 — The AI Agent Layer

AI agents operate at multiple levels:

| Level | What it does | Approval required |
|-------|-------------|-------------------|
| **Triage** | Categorize feedback, merge duplicates | None (automated) |
| **Scope** | Generate technical proposal from feedback | Developer reviews |
| **Implement** | Write code changes on a fork branch | Depends on governance |
| **Review** | Check security, style, test coverage | Configurable |
| **Deploy** | Deploy the fork to its domain | Configurable |
| **Promote** | Recommend merge-back based on metrics | Configurable |

Changes are limited to **allowed paths** configured per fork — the AI cannot modify arbitrary files.

### 2.4 — Governance Models

The developer chooses how decisions are made:

| Model | Who decides | Fork creation | Deploy | Merge-back |
|-------|------------|---------------|--------|------------|
| **Maintainer** | Developer | Manual | Manual | Manual |
| **Community** | Votes + metrics | Auto if threshold met | Auto after review | Manual |
| **Hands-off** | AI + metrics | Auto | Auto | Auto if metrics met |
| **Futarchy** | Token market | N/A | N/A | Market decides |

Governance is configurable per repository and can evolve over time.

### 2.5 — Fork Versioning

Forks need their own versioning primitive. Proposal: extend semver with fork metadata.

```
mainline:  1.2.3
fork A:    1.2.3+fork.a.1    (first deployment of fork A based on 1.2.3)
fork A:    1.2.3+fork.a.2    (second iteration)
fork A:    1.3.0+fork.a.1    (rebased onto mainline 1.3.0)
```

The `+fork.<name>.<n>` build metadata is ordered and comparable within the same fork lineage.

### 2.6 — Fork-of-Fork

Forks can themselves be forked. A fork of fork A becomes fork A/B, creating a tree:

```
main ──→ fork.a ──→ fork.a.x
    │         │
    │         └──→ fork.a.y
    │
    └──→ fork.b
```

The CLI tracks the full lineage and can diff any fork against any ancestor.

---

## 3. Technical architecture

### 3.1 — System diagram

```
                         monochange.dev
┌──────────────────────────────────────────────────────────────┐
│                                                              │
│  ┌─────────────────────┐    ┌────────────────────────────┐  │
│  │  monochange_app     │    │   Background Workers        │  │
│  │  (Leptos + axum)    │    │                              │  │
│  │                     │    │  ┌────────────────────────┐ │  │
│  │  • Dashboard        │    │  │  Feedback Triage Agent  │ │  │
│  │  • Roadmap viewer   │◄───┼──│  (categorize, dedup)   │ │  │
│  │  • Changelog        │    │  └────────────────────────┘ │  │
│  │  • Feedback queue   │    │                              │  │
│  │  • Fork manager     │    │  ┌────────────────────────┐ │  │
│  │  • Settings         │    │  │  Scoping Agent          │ │  │
│  └─────────┬───────────┘    │  │  (proposal generation)  │ │  │
│            │                │  └────────────────────────┘ │  │
│            │                │                              │  │
│            │                │  ┌────────────────────────┐ │  │
│            │                │  │  Implementation Agent    │ │  │
│            │                │  │  (code gen on fork)     │ │  │
│            │                │  └────────────────────────┘ │  │
│            │                │                              │  │
│            │                │  ┌────────────────────────┐ │  │
│            │                │  │  Review Agent           │ │  │
│            │                │  │  (security, style, test)│ │  │
│            │                │  └────────────────────────┘ │  │
│            │                │                              │  │
│            │                │  ┌────────────────────────┐ │  │
│            │                │  │  Deploy Agent           │ │  │
│            │                │  │  (CI/CD per fork)      │ │  │
│            │                │  └────────────────────────┘ │  │
│            │                └──────────────┬─────────────┘  │
│            │                               │                 │
│            └───────────────┬───────────────┘                 │
│                            ▼                                  │
│                   ┌──────────────────┐                       │
│                   │   PostgreSQL      │                       │
│                   │  (Fly Postgres)    │                       │
│                   └──────────────────┘                       │
└──────────────────────────────┼───────────────────────────────┘
                               │
     ┌─────────────────────────┼─────────────────────────┐
     ▼                         ▼                         ▼
┌──────────┐           ┌──────────────┐          ┌──────────────┐
│  GitHub   │           │  OpenRouter   │          │   Fork infra  │
│  API      │           │  (LLM API)    │          │  (Fly/CF/etc) │
└──────────┘           └──────────────┘          └──────────────┘
```

### 3.2 — Database schema (extensions for fork management)

The existing schema (users, organizations, installations, repositories) is extended with:

```rust
// Fork definition
#[derive(Debug, WeldsModel)]
#[welds(schema = "public", table = "forks")]
#[welds(BelongsTo(repository, super::repository::Repository, "repository_id"))]
pub struct Fork {
    #[welds(primary_key)]
    pub id: i32,
    pub repository_id: i32,
    pub name: String,               // "a", "b", "a.x" etc.
    pub parent_fork_id: Option<i32>, // null = fork of mainline
    pub deployment_mode: String,    // "feature_flag", "separate", "git_fork"
    pub git_branch: Option<String>,
    pub domain: Option<String>,     // e.g. "fork-a.example.com"
    pub status: String,             // "proposed", "implementing", "deployed", "promoted", "archived"
    pub governance: String,         // "maintainer", "community", "hands_off", "futarchy"
    pub allowed_paths: Option<String>, // JSON array of paths AI can modify
    pub metrics_json: Option<String>,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub deployed_at: Option<chrono::DateTime<chrono::Utc>>,
    pub merged_at: Option<chrono::DateTime<chrono::Utc>>,
}

// Changes made in a fork
#[derive(Debug, WeldsModel)]
#[welds(schema = "public", table = "fork_changes")]
#[welds(BelongsTo(fork, super::fork::Fork, "fork_id"))]
pub struct ForkChange {
    #[welds(primary_key)]
    pub id: i32,
    pub fork_id: i32,
    pub source_feedback_id: Option<i32>,
    pub changeset_filename: Option<String>,
    pub summary: String,
    pub ai_generated: bool,
    pub pr_number: Option<i64>,
    pub status: String,
    pub created_at: chrono::DateTime<chrono::Utc>,
}

// Promotion metrics
#[derive(Debug, WeldsModel)]
#[welds(schema = "public", table = "fork_metrics")]
#[welds(BelongsTo(fork, super::fork::Fork, "fork_id"))]
pub struct ForkMetrics {
    #[welds(primary_key)]
    pub id: i32,
    pub fork_id: i32,
    pub active_users: i32,
    pub total_sessions: i32,
    pub feedback_score: f64,
    pub vote_count: i32,
    pub merge_token_price: Option<f64>, // for futarchy
    pub recorded_at: chrono::DateTime<chrono::Utc>,
}
```

### 3.3 — monochange.toml fork extensions

```toml
# Existing sections...
[package.my-app]
path = "apps/web"

# Fork configuration
[forks]
# Default governance for this repo
default_governance = "maintainer"
# Deployment platform for forks
deployment_platform = "fly.io"  # or "vercel", "cloudflare", "custom"

# Example fork definition
[forks.dark-mode]
deployment_mode = "separate"
domain = "dark-mode.myapp.monochange.dev"
allowed_paths = ["apps/web/src/styles/**", "apps/web/src/components/theme/**"]
governance = "community"
auto_deploy = false
required_approvals = 2

[forks.beta-redesign]
deployment_mode = "feature_flag"
feature_flag_key = "BETA_REDESIGN"
allowed_paths = ["apps/web/src/**"]
governance = "maintainer"
```

---

## 4. Fork lifecycle (detailed)

### Stage 1: Proposal
```
User feedback → AI categorizes → AI scopes into proposal
→ Proposal appears in developer queue
→ Developer: accept / reject / request changes
```

### Stage 2: Fork Creation
```
Proposal accepted → AI creates fork configuration
→ monochange.toml updated with [forks.<name>]
→ Git branch created (if git fork mode)
→ AI generates changeset for the fork
```

### Stage 3: Implementation
```
AI agent generates code changes → only within allowed_paths
→ Changes committed to fork branch
→ If governance = maintainer: PR opened, developer reviews
→ If governance = community: PR opened, N approvals needed
→ If governance = hands_off: auto-merge after review agent passes
→ CI runs on fork branch (tests, lint, security scan)
```

### Stage 4: Deployment
```
CI passes → fork deployed to domain / feature flag activated
→ Users notified that fork is available
→ Fork metrics tracking begins
```

### Stage 5: Observation
```
Users use the fork → metrics collected:
  - active users, sessions
  - feedback/votes specifically on the fork
  - engagement metrics (time on page, feature usage)
  - if futarchy: token price
→ Metrics dashboard visible on fork page
```

### Stage 6: Decision
```
Promotion criteria evaluated:
  - Manual: developer clicks "promote"
  - Community: vote threshold + metrics threshold met
  - Hands-off: AI determines based on metrics
  - Futarchy: market resolves YES/NO
```

### Stage 7: Merge-back
```
Fork promoted → merge PR opened against mainline
→ AI attempts rebase/merge
→ If conflicts: AI suggests resolution, human reviews
→ If clean: merge proceeds
→ Fork archived or kept as long-lived branch
→ monochange.toml updated to reflect merged changes
```

---

## 5. Merge strategy (my recommendation)

When a fork is promoted, merge isn't clean because:
1. The fork diverged from a past point in mainline
2. Mainline has moved on
3. The fork may have its own monochange.toml

**Proposed strategy: Rebase-then-squash**

```
1. Fork branch rebased onto current mainline HEAD
   → AI-assisted conflict resolution
   → Human reviews if conflicts are non-trivial

2. Rebased fork goes through full CI on the rebased state

3. Squash-merge into mainline (single commit):
   "feat: merge fork.dark-mode (2,341 users, 94% approval)"
   
4. Fork's changeset entries are migrated to mainline .changeset/
   → Each fork change gets its own changeset file
   → `caused_by` field links back to the fork

5. monochange.toml cleaned up:
   → Fork section archived as [forks.dark-mode.archived]
   → Package versions updated if fork bumped versions
   → Lockfile refreshed
```

---

## 6. Phased roadmap from current scaffold

### Phase 0: Foundation ✅ COMPLETE
- Leptos SSR + WASM build pipeline
- Tailwind v4 + dark/light mode
- Server functions (auth, repos, feedback, roadmap, ai)
- E2E tests with playwright-rs
- CI workflow

### Phase 1: Core SaaS — Weeks 1–8
| Week | Deliverable |
|------|-------------|
| 1–2 | Database models (Welds) + migrations for users, orgs, installations, repos |
| 3–4 | GitHub OAuth flow (real sign-in, JWT sessions, cookie management) |
| 5–6 | Dashboard — connected repos, installation management, settings pages |
| 7–8 | GitHub App webhook receiver → store installations, sync repos |

### Phase 2: Feedback System — Weeks 9–14
| Week | Deliverable |
|------|-------------|
| 9–10 | Feedback form management CRUD (create form, configure fields, embed code) |
| 11–12 | JS embed widget (vanilla TS, shadow DOM, theme support) |
| 13–14 | AI triage agent (categorize, dedup, generate proposals) |

### Phase 3: AI Scoping — Weeks 15–20
| Week | Deliverable |
|------|-------------|
| 15–16 | OpenRouter integration — structured prompts, JSON output parsing |
| 17–18 | Scoping agent — feedback → technical proposal with package/bump/effort |
| 19–20 | Public roadmap page with voting |

### Phase 4: Fork Engine — Weeks 21–32
| Week | Deliverable |
|------|-------------|
| 21–22 | Fork data model + monochange.toml fork extensions |
| 23–24 | Fork creation workflow (git branch, config, changeset) |
| 25–26 | Implementation agent (AI generates code on fork) |
| 27–28 | Review agent (automatic security/style/test checks) |
| 29–30 | Deployment agent (CI/CD per fork, domain provisioning) |
| 31–32 | Fork metrics collection + dashboard |

### Phase 5: Merge-back — Weeks 33–40
| Week | Deliverable |
|------|-------------|
| 33–34 | Promotion criteria engine (multi-signal: users, votes, metrics) |
| 35–36 | Rebase-then-squash merge workflow |
| 37–38 | Fork-of-fork support (tree management in monochange.toml) |
| 39–40 | Futarchy token integration (optional, research-dependent) |

### Phase 6: Scale & Platform — Weeks 41+
- Multi-tenant isolation hardening
- Usage-based billing
- GitLab support
- Mobile feature-flag mode (CodePush integration)
- Enterprise SSO, audit logs

---

## 7. Open questions & risks

1. **Security of AI-generated code** — Even with `allowed_paths`, AI can inject malicious code. The review agent must do static analysis + runtime sandboxing. This is the hardest problem.

2. **Database isolation on forks** — For "separate deployment" mode, does each fork get its own database? This affects cost, data consistency, and merge complexity.

3. **monochange.toml merge conflicts** — When a fork changes `monochange.toml` (adds packages, changes groups), merging back requires structured merging, not text diff.

4. **Futarchy complexity** — Token-based governance is conceptually elegant but legally and practically complex. I'd keep it as a research item, not MVP.

5. **Rebase fidelity** — The longer a fork lives, the harder the rebase. There needs to be a "stale fork" policy (auto-archive if N weeks without rebase).

6. **Who hosts the forks?** — If monochange manages the deployments (e.g., on Fly.io), you're eating the infra cost. If users host their own, the integration complexity is higher for them. This needs a clear pricing model.
