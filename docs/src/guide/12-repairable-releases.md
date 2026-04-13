# Repairable releases

`mc repair-release` is for the stressful moment right after a release when you discover that a few follow-up commits still need to be part of that release.

Examples:

- a packaging file was missing from the release branch
- generated artifacts were wrong
- a release automation step succeeded, but the tagged commit needs one or two immediate fixes before the release should stand

monochange solves that by storing a durable release declaration in git history and then using that declaration to move the whole release set forward together.

## The two artifacts: release manifest vs release record

monochange now has two related but different release artifacts:

| Artifact                                                      | What it means                                  | When it exists                                    | What it is for                                                                    |
| ------------------------------------------------------------- | ---------------------------------------------- | ------------------------------------------------- | --------------------------------------------------------------------------------- |
| cached release manifest (`.monochange/release-manifest.json`) | what monochange is preparing right now         | during command execution and cached locally       | CI, MCP/server consumers, previews, downstream automation, and AI/agent workflows |
| `ReleaseRecord`                                               | what this release commit historically declared | inside the monochange-managed release commit body | later inspection and repair from git history                                      |

Plain-language summary:

- manifest = "what monochange is preparing right now"
- release record = "what this release commit historically declared"

If you prefer the emphasized version:

- **manifest** = "what monochange is preparing right now"
- **release record** = "what this release commit historically declared"

The important consequence is that `ReleaseRecord` does **not** replace the cached release manifest.

Use the manifest when you want execution-time automation. Use the release record when you want history-time inspection or repair.

## Where the release record lives

monochange writes the durable `ReleaseRecord` into the body of the monochange-managed release commit.

That means the repair anchor travels with git history itself instead of living in a mutable receipt file somewhere in the repository tree.

The release commit body contains:

1. a compact human-readable release summary
2. a reserved monochange release-record block with structured JSON

## How monochange finds a release later

Use `mc release-record` when you want to inspect the durable release declaration for a tag or a newer commit built on top of that release.

```bash
mc release-record --from v1.2.3
mc release-record --from HEAD --format json
```

monochange will:

1. resolve the supplied ref to a commit
2. walk first-parent ancestry
3. stop at the first valid monochange `ReleaseRecord`
4. report the release commit that declared it plus the distance from the input ref

That lets you inspect a release directly from its tag or from later fix commits.

## Repairing a recent release

Use `mc repair-release` when you want to move a recent release forward to a later commit.

```bash
mc repair-release --from v1.2.3 --target HEAD --dry-run
mc repair-release --from v1.2.3 --target HEAD
```

The command does the heavy lifting for you:

1. finds the canonical release record from history
2. derives the full release set from that record
3. validates descendant-only safety rules by default
4. previews the retarget plan in dry-run mode
5. moves the whole tag set together when run for real
6. syncs hosted release state when the provider supports it

### Dry-run first

`repair-release` is intentionally a dry-run-friendly workflow.

Use dry-run to see:

- the release record monochange found
- the target commit
- which tags will move
- whether the target is a descendant of the original release commit
- whether hosted-release sync will run

```bash
mc repair-release --from v1.2.3 --target HEAD --dry-run --format json
```

## Example workflow

A typical repair flow looks like this:

1. monochange creates a release request commit with an embedded release record.
2. That release is tagged and published.
3. You add a follow-up fix commit or two.
4. You inspect the durable history record:

```bash
mc release-record --from v1.2.3
```

5. You preview the repair:

```bash
mc repair-release --from v1.2.3 --target HEAD --dry-run
```

6. You execute the repair:

```bash
mc repair-release --from v1.2.3 --target HEAD
```

## What `repair-release` changes

`repair-release` is focused and narrow. It changes:

- the release-set git tags derived from the durable release record
- hosted source-provider release state when supported by the provider integration

It does **not**:

- rewrite the original release commit
- rewrite the historical release record block
- regenerate a new release plan from scratch
- automatically republish immutable registry artifacts

## When to use this vs publish a new patch release

Use `repair-release` for **just-created source/provider releases** when the right fix is to move the release tags forward to a later commit.

Prefer publishing a new patch release when:

- immutable registry artifacts have already been published and consumers may already be relying on them
- you need a new externally visible version instead of retargeting an existing source release
- the release is no longer an immediate post-release repair situation

If you are under pressure, the rule of thumb is simple:

- if you need to fix the just-created source release itself, use `repair-release`
- if you need a new immutable published artifact, cut a new patch release

## Configuration and step model

The user-facing command is:

```bash
mc repair-release --from v1.2.3 --target HEAD
```

The underlying built-in step is `RetargetRelease`.

That means you can also compose it into custom CLI workflows and then reference its structured outputs through `retarget.*` in later command steps.

The main fields exposed there are:

- `retarget.from`
- `retarget.target`
- `retarget.record_commit`
- `retarget.resolved_from_commit`
- `retarget.distance`
- `retarget.tags`
- `retarget.provider_results`
- `retarget.status`

## Provider scope in v1

GitHub is the first provider with release retarget sync support.

When provider sync is unsupported, monochange reports that clearly in dry-run and real execution paths rather than pretending the operation completed.

## Keep using release manifests for automation

The new history-oriented repair workflow does not remove the execution-time manifest workflow.

Keep using the cached manifest JSON from `PrepareRelease` when you want:

- machine-readable release plans in CI
- MCP/server responses for assistants
- deterministic previews for downstream automation
- a stable execution-time snapshot of what monochange is about to do

Use `ReleaseRecord` and `repair-release` when you want to inspect or repair a release later from git history.
