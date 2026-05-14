# Hosted OIDC release commits

**Status**: Planned\
**Created**: 2026-05-14\
**Branch**: `feat/monochange-app-planning`\
**Owner**: monochange app / release automation

---

## Problem statement

monochange release PR workflows need to update a release branch with generated release files, then allow the release PR to run normal CI before it is merged and published.

The current choices are not good enough for a generic user-facing product:

- `GITHUB_TOKEN` is safe and repository-scoped, but branch updates made with it do not trigger the follow-up workflows we need to validate the generated release PR.
- A personal access token can trigger workflows, but it is user-specific, hard to productize, and creates unbranded/unverified commits unless every user solves commit identity/signing separately.
- Local `git commit && git push` from CI is difficult to make work with repositories that require verified commits.

We want release PR commits to be:

1. Created by the monochange GitHub App/bot identity.
2. Verified by GitHub.
3. Able to trigger normal workflows.
4. Available to any repository that installs the monochange GitHub App.
5. Usable without storing a long-lived monochange API token in GitHub Actions.

---

## Product goal

Add a hosted commit backend for `CommitRelease` so GitHub Actions can ask `monochange.dev` to create the release commit on its behalf.

The GitHub Action still computes the release locally with monochange, but delegates commit creation to the hosted monochange app:

```text
GitHub Actions job
  ├─ checkout repo
  ├─ mc release-pr / PrepareRelease
  ├─ CommitRelease(commit_backend = "hosted")
  │    ├─ gather changed release files + commit metadata
  │    ├─ request GitHub Actions OIDC token
  │    └─ POST commit request to monochange.dev
  └─ OpenReleaseRequest / poll existing PR status

monochange.dev
  ├─ verify GitHub Actions OIDC token
  ├─ verify repository is connected and installed
  ├─ mint GitHub App installation token
  ├─ create blobs/tree/commit through GitHub Git Database API
  ├─ update release branch ref
  └─ return commit SHA + verification status
```

The generated commit should appear as the monochange app/bot and satisfy verified-commit branch protection.

---

## Key decisions

| # | Decision                                  | Choice                                                                                       |
| - | ----------------------------------------- | -------------------------------------------------------------------------------------------- |
| 1 | Authentication from Actions to hosted app | Support both GitHub Actions OIDC and `MONOCHANGE_TOKEN`; recommend OIDC                      |
| 2 | GitHub commit identity                    | monochange GitHub App installation token                                                     |
| 3 | Commit creation mechanism                 | GitHub Git Database API (`blobs`, `trees`, `commits`, `refs`)                                |
| 4 | CLI surface                               | Add hosted mode to `CommitRelease` rather than a separate command                            |
| 5 | Local behavior                            | Existing local `CommitRelease` remains default and unchanged                                 |
| 6 | Cross-provider design                     | Hosted commit request is provider-neutral; GitHub is first implementation                    |
| 7 | GitLab/Forges                             | Future hosted provider adapters implement the same hosted commit contract                    |
| 8 | Secrets                                   | GitHub App private key/webhook secret live only in monochange app deployment                 |
| 9 | User setup                                | User installs monochange GitHub App and enables the hosted commit backend in workflow config |

---

## Proposed user workflow

### One-time repository setup

1. User signs in to `monochange.dev` with GitHub OAuth.
2. User installs the monochange GitHub App on an organization or repository.
3. monochange stores the installation and allowed repositories.
4. User enables hosted release commits in `monochange.toml` or the CLI workflow config.

Example config shape to refine during implementation:

```toml
[cli.release-pr]
steps = [
	{ type = "PrepareRelease" },
	{ type = "CommitRelease", commit_backend = "hosted" },
	{ type = "OpenReleaseRequest" },
]
```

Optional explicit endpoint/audience configuration:

```toml
[hosted]
url = "https://monochange.dev"
oidc_audience = "monochange.dev"
```

### GitHub Actions workflow

Recommended workflow: grant OIDC permission and normal read/write permissions for local preparation. This path does **not** need a PAT or monochange API token. For CI systems that cannot use OIDC yet, hosted mode may also accept `MONOCHANGE_TOKEN`, but docs should present that as a fallback.

```yaml
permissions:
  contents: write
  pull-requests: write
  id-token: write

steps:
  - uses: actions/checkout@v6
    with:
      fetch-depth: 0
  - run: mc release-pr
```

The monochange CLI will request an OIDC token from GitHub Actions when it reaches hosted `CommitRelease` unless `hosted_auth = "token"` is configured or OIDC is unavailable and token fallback is explicitly allowed.

---

## OIDC authentication design

### In the GitHub Actions runner

When `CommitRelease(commit_backend = "hosted")` runs, the CLI:

1. Reads `ACTIONS_ID_TOKEN_REQUEST_URL` and `ACTIONS_ID_TOKEN_REQUEST_TOKEN`.
2. Requests an OIDC JWT with audience `monochange.dev` or a configurable audience.
3. Sends the JWT to `monochange.dev` in the hosted commit request.

GitHub Actions users should not need to create or rotate a long-lived `MONOCHANGE_TOKEN` for CI. `MONOCHANGE_TOKEN` remains available as a fallback for non-OIDC environments and emergency compatibility.

### In monochange.dev

The app verifies the OIDC token by:

1. Fetching GitHub's OIDC JWKS.
2. Validating JWT signature.
3. Validating issuer: `https://token.actions.githubusercontent.com`.
4. Validating audience matches the configured monochange audience.
5. Validating important claims:
   - `repository`
   - `repository_id`
   - `repository_owner`
   - `repository_owner_id`
   - `ref`
   - `sha`
   - `workflow`
   - `job_workflow_ref` when available
   - `run_id`
   - `run_attempt`
6. Mapping the repository claim to a connected repository in the app database.
7. Verifying that the connected repository has a GitHub App installation.
8. Optionally enforcing branch/workflow policy from repository settings.

The OIDC token proves the request came from a GitHub Actions run for that repository. The GitHub App installation proves monochange is authorized to write to that repository.

---

## Hosted commit request contract

The CLI should send a compact, deterministic request that contains all information the hosted service needs to recreate the release commit.

Draft shape:

```json
{
	"provider": "github",
	"repository": "owner/name",
	"baseBranch": "main",
	"headBranch": "monochange/release/release-pr",
	"expectedHeadSha": "...",
	"commit": {
		"subject": "chore(release): prepare release",
		"body": "..."
	},
	"files": [
		{
			"path": "CHANGELOG.md",
			"contentBase64": "...",
			"mode": "100644"
		},
		{
			"path": ".monochange/releases/2026-05-14.json",
			"contentBase64": "...",
			"mode": "100644"
		}
	],
	"deletions": [
		".changeset/example.md"
	],
	"releaseRecord": {
		"path": ".monochange/releases/...json",
		"sha256": "..."
	},
	"idempotencyKey": "github-run-id:run-attempt:command-name"
}
```

Notes:

- Include only release-managed paths.
- Use base64 for file contents to avoid encoding ambiguity.
- Include file mode so symlinks/executable bits can be handled later.
- Include `expectedHeadSha` to prevent overwriting a branch that moved after local preparation.
- Include an idempotency key so reruns can safely retry.

---

## Hosted commit response contract

Draft response:

```json
{
	"provider": "github",
	"repository": "owner/name",
	"headBranch": "monochange/release/release-pr",
	"commitSha": "...",
	"verified": true,
	"verificationReason": "valid",
	"triggeredWorkflows": true,
	"pullRequest": {
		"number": 505,
		"url": "https://github.com/owner/name/pull/505"
	}
}
```

`CommitReleaseReport` should gain fields for hosted mode, e.g.:

- `commit`
- `verified`
- `verification_reason`
- `commit_backend`
- `hosted_request_id`

---

## CLI changes

### `monochange_core`

- [x] Add a `commit_backend` field to `CliStepDefinition::CommitRelease`.
- [x] Supported values: `local` and `hosted`.
- [x] Default: `local` for backwards compatibility.
- [ ] Keep existing `no_verify` and `update_release_json` behavior for local mode.
- [x] Define expected input kind for `commit_backend`.
- [ ] Update schema generation and snapshots.

Possible enum:

```rust
pub enum CommitReleaseBackend {
	Local,
	Hosted,
}
```

### `monochange`

- [ ] Split current `commit_release` into:
  - local release file validation/preparation
  - local git commit implementation
  - hosted commit request builder
- [x] Add GitHub Actions OIDC token acquisition helper.
- [ ] Add hosted API client for `POST /api/release-commits`.
- [ ] In hosted mode, do not run local `git commit`.
- [ ] In hosted mode, still validate and optionally update release record JSON before packaging files.
- [ ] Update CLI output and markdown reports to show hosted commit status.

### `monochange_github`

Existing GitHub Git Database API code already creates blobs, trees, commits, and refs for verified release PR commits. Reuse or extract this into a provider adapter that accepts a hosted commit request.

- [ ] Extract reusable commit-creation primitive from current release PR verified-commit flow.
- [ ] Support installation-token clients, not only env token clients.
- [ ] Preserve verification checks (`verification.verified == true`).
- [ ] Preserve branch-moved safety checks.

---

## monochange_app changes

### GitHub App configuration

These are created by us in GitHub and deployed as monochange app secrets:

- `MONOCHANGE_GITHUB_APP_ID`
- `MONOCHANGE_GITHUB_APP_PRIVATE_KEY`
- `MONOCHANGE_GITHUB_APP_WEBHOOK_SECRET`

They are **not** user-provided. Users install our GitHub App; they do not create their own bot.

- [ ] Add runtime config loader for GitHub App settings.
- [ ] Fail clearly when hosted commit routes are enabled without app credentials.
- [ ] Document Fly.io secret setup.

### Installation tracking

- [ ] Add/finish GitHub App installation webhook endpoint.
- [ ] Verify webhook signatures using `MONOCHANGE_GITHUB_APP_WEBHOOK_SECRET`.
- [ ] Store installation id, account id/login/type, repository ids, permissions, and events.
- [ ] Update repositories when installation is created, edited, suspended, unsuspended, or deleted.
- [ ] Surface connected repositories in the dashboard.

### OIDC verification

- [ ] Add GitHub Actions OIDC verifier module.
- [ ] Cache GitHub JWKS with expiry.
- [ ] Validate issuer/audience/signature.
- [ ] Validate repository claims against connected repositories.
- [ ] Add repository-level policy checks for allowed branches/workflows.
- [ ] Log run metadata for auditability.

### Hosted release commit API

Endpoint draft:

```text
POST /api/repositories/{repository_id}/release-commits
Authorization: Bearer <GitHub Actions OIDC JWT>
```

Alternative if repository id is not known locally:

```text
POST /api/release-commits/github/{owner}/{repo}
Authorization: Bearer <GitHub Actions OIDC JWT>
```

- [x] Authenticate with OIDC.
- [ ] Resolve repository and installation.
- [ ] Enforce path allowlist.
- [ ] Enforce expected branch SHA.
- [ ] Mint GitHub App installation token.
- [ ] Create GitHub commit through Git Database API.
- [ ] Verify commit is verified.
- [ ] Update release branch ref.
- [ ] Return hosted commit response.
- [ ] Store audit record.

### Database additions

Add tables/models for:

- [ ] app installations / repository installation mapping, if existing models are not enough.
- [ ] hosted commit requests.
- [ ] hosted commit audit events.
- [ ] OIDC run metadata.
- [ ] idempotency keys.

Example hosted commit audit fields:

- repository id
- installation id
- provider
- workflow run id / attempt
- actor
- source SHA
- target branch
- expected head SHA
- resulting commit SHA
- verification result
- request hash
- created at

---

## Security rules

- [ ] No long-lived CI token required for first version.
- [ ] Do not accept arbitrary file paths; only release-managed paths are allowed by default.
- [ ] Reject absolute paths, parent traversal, `.git`, workflow mutation unless explicitly allowed.
- [ ] Require expected SHA for branch updates.
- [ ] Use idempotency keys for retries.
- [ ] Rate limit hosted commit endpoint by repository and installation.
- [ ] Store enough audit metadata to explain who/what caused a commit.
- [ ] Keep GitHub App private key only in server-side secret storage.
- [ ] Never return installation tokens to users or Actions.

---

## GitLab and other providers

The hosted commit concept should be provider-neutral even though GitHub lands first.

Define a server-side trait similar to:

```rust
trait HostedCommitProvider {
	async fn create_release_commit(&self, request: HostedCommitRequest) -> HostedCommitResult;
}
```

Provider-specific auth options:

| Provider      | CI-to-monochange auth                                    | Hosted commit authority                                        |
| ------------- | -------------------------------------------------------- | -------------------------------------------------------------- |
| GitHub        | GitHub Actions OIDC                                      | monochange GitHub App installation token                       |
| GitLab        | Future GitLab OIDC/JWT or project job token verification | GitLab application/bot/project token depending on capabilities |
| Forgejo/Gitea | Future CI OIDC or configured token exchange              | App/bot token where supported                                  |

GitLab may not have the same verified-commit behavior as GitHub Apps. The generic contract should return provider-specific verification/identity metadata rather than pretending all providers behave like GitHub.

---

## Implementation phases

### Phase 1 — Design and schema

- [x] Add this plan.
- [ ] Decide final config field name (`commit_backend`, `committer`, or `commit_mode`).
- [x] Add core enum/schema docs.
- [ ] Add CLI help text.

### Phase 2 — CLI hosted request builder

- [ ] Build deterministic hosted commit payload from prepared release files.
- [ ] Add OIDC token acquisition in GitHub Actions.
- [ ] Add hosted API client.
- [ ] Add dry-run output showing what would be sent.
- [ ] Unit test payload construction.

### Phase 3 — App OIDC verifier

- [ ] Implement GitHub OIDC JWT verification.
- [ ] Add JWKS cache.
- [ ] Add repository claim mapping.
- [ ] Add tests with fixture JWT/JWKS data.

### Phase 4 — GitHub App installation tokens

- [ ] Add GitHub App config.
- [ ] Add JWT creation for app auth.
- [ ] Exchange app JWT for installation token.
- [ ] Store/update installation mappings from webhooks.
- [ ] Test token minting with mocked GitHub API.

### Phase 5 — Hosted GitHub commit endpoint

- [ ] Implement endpoint and request validation.
- [ ] Reuse/extract Git Database API commit creation.
- [ ] Enforce verified commit result.
- [ ] Add idempotency and audit persistence.
- [ ] Add integration tests with mocked GitHub API.

### Phase 6 — Release PR workflow integration

- [ ] Update sample release PR workflow to use `id-token: write`.
- [ ] Update release PR docs.
- [ ] Test on this repository by replacing personal token usage.
- [ ] Confirm generated commit triggers workflows.
- [ ] Confirm generated commit satisfies verified-commit requirement.

### Phase 7 — Production rollout

- [ ] Create production monochange GitHub App.
- [ ] Configure Fly.io secrets.
- [ ] Add install link to dashboard.
- [ ] Add repository setup instructions.
- [ ] Add failure diagnostics for missing install, bad OIDC audience, moved branch, unverified commit.

---

## Acceptance criteria

- [ ] A repository can run the release PR action without a PAT or `MONOCHANGE_TOKEN`.
- [ ] The action authenticates to monochange.dev with GitHub Actions OIDC.
- [ ] monochange.dev creates the release commit using the monochange GitHub App installation.
- [ ] The commit is shown as verified by GitHub.
- [ ] The commit triggers normal repository workflows.
- [ ] Branch movement is detected and rejected safely.
- [ ] Existing local `CommitRelease` users are unaffected.
- [ ] The design does not hard-code monochange/monochange; it works for any connected repository.

---

## Open questions

- [ ] Final config field name: `commit_backend`, `commit_mode`, or `committer`?
- [ ] Should hosted commit creation also open/update the PR, or should `OpenReleaseRequest` remain separate?
- [ ] Should hosted mode support a fallback long-lived token for non-GitHub CI, or wait for provider-specific OIDC?
- [ ] Which paths are release-managed by default, and how do users extend them safely?
- [ ] Should OIDC policy allow only default branch workflows, or any workflow in the connected repository?
- [ ] What exact GitHub verification response do GitHub App-created Git Database API commits return in production?

---

## Notes while executing

Use this section to record implementation discoveries and decision changes as the plan is executed.

- 2026-05-14: Decided to prioritize GitHub Actions OIDC immediately while still allowing `MONOCHANGE_TOKEN` as a fallback.
- 2026-05-14: Hosted app secrets (`MONOCHANGE_GITHUB_APP_ID`, private key, webhook secret) are owned by monochange deployment, not by individual users.

- 2026-05-14: Added core `CommitReleaseBackend` and `HostedCommitAuth` config fields. `local` remains default; hosted auth supports `auto`, `oidc`, and `token`, with OIDC recommended.

- 2026-05-14: Added the CLI hosted commit client path: hosted mode now prepares release files locally, builds a provider-neutral request payload, authenticates with GitHub Actions OIDC or `MONOCHANGE_TOKEN`, and posts to `/api/release-commits`.
