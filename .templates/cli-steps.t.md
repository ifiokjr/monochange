<!-- {@cliStepReferenceOverview} -->

monochange CLI commands are built from ordered `[[cli.<command>.steps]]` entries.

A step is the smallest execution unit in a monochange workflow. Some steps are **standalone** (`Validate`, `Discover`, `AffectedPackages`, `DiagnoseChangesets`, `RetargetRelease`). Others are **stateful** and build on the result of an earlier `PrepareRelease` step (`CommitRelease`, `PublishRelease`, `OpenReleaseRequest`, and `CommentReleasedIssues`). `PrepareRelease` also refreshes the cached `.monochange/release-manifest.json` artifact exposed to later steps as `manifest.path`.

When you design a command, think in terms of:

1. **what state the command needs**
2. **which step produces that state**
3. **which later step consumes it**
4. **what side effects are acceptable** in normal mode vs `--dry-run`

The reference pages in this section document each built-in step with:

- what the step does
- why you would choose it over a shell-only `Command`
- which inputs it accepts
- what prerequisite state it needs
- what it contributes to later steps
- examples of how it composes into full workflows

<!-- {/cliStepReferenceOverview} -->

<!-- {@cliStepReferenceChoosingGuide} -->

| Step                    | Use it when you want to…                                                  | Requires previous step?          | Typical follow-up                                                                           |
| ----------------------- | ------------------------------------------------------------------------- | -------------------------------- | ------------------------------------------------------------------------------------------- |
| `Validate`              | fail fast on invalid config, groups, or changesets                        | no                               | CI gate or local preflight                                                                  |
| `Discover`              | inspect normalized package discovery across ecosystems                    | no                               | local inspection, debug commands                                                            |
| `CreateChangeFile`      | author a `.changeset/*.md` file from CLI inputs                           | no                               | run independently, or before planning                                                       |
| `PrepareRelease`        | build the release result, update files, and refresh the cached manifest   | no                               | `CommitRelease`, `PublishRelease`, `OpenReleaseRequest`, `CommentReleasedIssues`, `Command` |
| `DisplayVersions`       | display planned package and group versions without mutating release files | no                               | `PrepareRelease`                                                                            |
| `CommitRelease`         | create a local release commit with an embedded `ReleaseRecord`            | `PrepareRelease`                 | `OpenReleaseRequest`, manual review, custom `Command`                                       |
| `PublishRelease`        | create or update hosted provider releases                                 | `PrepareRelease` + `[source]`    | `CommentReleasedIssues`, custom notification commands                                       |
| `OpenReleaseRequest`    | create or update a hosted release PR/MR                                   | `PrepareRelease` + `[source]`    | provider review, follow-up `Command` steps                                                  |
| `PlanPublishRateLimits` | plan package-registry publish work against known rate limits              | no                               | `PublishPackages`, `PlaceholderPublish`                                                     |
| `PlaceholderPublish`    | publish `0.0.0` placeholder versions for missing registry packages        | no                               | normally before `PublishPackages`                                                           |
| `PublishPackages`       | publish package versions to registries using built-in ecosystem workflows | `PrepareRelease`                 | custom `Command` steps using `publish.*`                                                    |
| `CommentReleasedIssues` | post release follow-up comments to closed issues                          | `PrepareRelease` + GitHub source | normally after `PublishRelease`                                                             |
| `AffectedPackages`      | evaluate changeset coverage for changed files                             | no                               | CI enforcement, custom failure messaging                                                    |
| `DiagnoseChangesets`    | inspect changeset context, commit provenance, and linked review metadata  | no                               | local debugging, CI inspection                                                              |
| `RetargetRelease`       | repair a recent release by moving its tag set                             | no                               | custom `Command` steps using `retarget.*`                                                   |
| `Command`               | run arbitrary shell/program commands with monochange context              | depends on your workflow         | any external tool                                                                           |

<!-- {/cliStepReferenceChoosingGuide} -->

<!-- {@cliStepValidateExample} -->

```toml
[cli.validate]
help_text = "Validate monochange configuration and changesets"

[[cli.validate.steps]]
type = "Validate"
```

<!-- {/cliStepValidateExample} -->

<!-- {@cliStepDiscoverExample} -->

```toml
[cli.discover]
help_text = "Discover packages across supported ecosystems"

[[cli.discover.inputs]]
name = "format"
type = "choice"
choices = ["text", "json"]
default = "text"

[[cli.discover.steps]]
type = "Discover"
```

<!-- {/cliStepDiscoverExample} -->

<!-- {@cliStepCreateChangeFileExample} -->

```toml
[cli.change]
help_text = "Create a change file for one or more packages"

[[cli.change.inputs]]
name = "interactive"
type = "boolean"
short = "i"

[[cli.change.inputs]]
name = "package"
type = "string_list"

[[cli.change.inputs]]
name = "bump"
type = "choice"
choices = ["none", "patch", "minor", "major"]
default = "patch"

[[cli.change.inputs]]
name = "version"
type = "string"

[[cli.change.inputs]]
name = "type"
type = "string"

[[cli.change.inputs]]
name = "caused_by"
type = "string_list"

[[cli.change.inputs]]
name = "reason"
type = "string"

[[cli.change.inputs]]
name = "details"
type = "string"

[[cli.change.inputs]]
name = "output"
type = "string"

[[cli.change.steps]]
type = "CreateChangeFile"
```

<!-- {/cliStepCreateChangeFileExample} -->

<!-- {@cliStepPrepareReleaseExample} -->

```toml
[cli.release]
help_text = "Prepare a release from discovered change files"

[[cli.release.inputs]]
name = "format"
type = "choice"
choices = ["text", "json"]
default = "text"

[[cli.release.steps]]
type = "PrepareRelease"
```

<!-- {/cliStepPrepareReleaseExample} -->

<!-- {@cliStepCommitReleaseExample} -->

```toml
[cli.commit-release]
help_text = "Prepare a release and create a local release commit"

[[cli.commit-release.inputs]]
name = "format"
type = "choice"
choices = ["text", "json"]
default = "text"

[[cli.commit-release.steps]]
type = "PrepareRelease"

[[cli.commit-release.steps]]
type = "CommitRelease"
```

<!-- {/cliStepCommitReleaseExample} -->

<!-- {@cliStepPublishReleaseExample} -->

```toml
[cli.publish-release]
help_text = "Prepare a release and publish hosted releases"

[[cli.publish-release.inputs]]
name = "format"
type = "choice"
choices = ["text", "json"]
default = "text"

[[cli.publish-release.steps]]
type = "PrepareRelease"

[[cli.publish-release.steps]]
type = "PublishRelease"
```

<!-- {/cliStepPublishReleaseExample} -->

<!-- {@cliStepOpenReleaseRequestExample} -->

```toml
[cli.release-pr]
help_text = "Prepare a release and open or update a release request"

[[cli.release-pr.inputs]]
name = "format"
type = "choice"
choices = ["text", "json"]
default = "text"

[[cli.release-pr.steps]]
type = "PrepareRelease"

[[cli.release-pr.steps]]
type = "OpenReleaseRequest"
```

<!-- {/cliStepOpenReleaseRequestExample} -->

<!-- {@cliStepCommentReleasedIssuesExample} -->

```toml
[cli.publish-and-comment]
help_text = "Publish a release and comment on linked issues"

[[cli.publish-and-comment.inputs]]
name = "format"
type = "choice"
choices = ["text", "json"]
default = "text"

[[cli.publish-and-comment.steps]]
type = "PrepareRelease"

[[cli.publish-and-comment.steps]]
type = "PublishRelease"

[[cli.publish-and-comment.steps]]
type = "CommentReleasedIssues"
```

<!-- {/cliStepCommentReleasedIssuesExample} -->

<!-- {@cliStepAffectedPackagesExample} -->

```toml
[cli.affected]
help_text = "Evaluate pull-request changeset policy"

[[cli.affected.inputs]]
name = "format"
type = "choice"
choices = ["text", "json"]
default = "text"

[[cli.affected.inputs]]
name = "changed_paths"
type = "string_list"
required = true

[[cli.affected.inputs]]
name = "label"
type = "string_list"

[[cli.affected.inputs]]
name = "verify"
type = "boolean"

[[cli.affected.steps]]
type = "AffectedPackages"
```

<!-- {/cliStepAffectedPackagesExample} -->

<!-- {@cliStepDiagnoseChangesetsExample} -->

```toml
[cli.diagnostics]
help_text = "Inspect changeset context and provenance"

[[cli.diagnostics.inputs]]
name = "format"
type = "choice"
choices = ["text", "json"]
default = "text"

[[cli.diagnostics.inputs]]
name = "changeset"
type = "string_list"

[[cli.diagnostics.steps]]
type = "DiagnoseChangesets"
```

<!-- {/cliStepDiagnoseChangesetsExample} -->

<!-- {@cliStepRetargetReleaseExample} -->

```toml
[cli.repair-release]
help_text = "Repair a recent release by retargeting its tags"

[[cli.repair-release.inputs]]
name = "from"
type = "string"
required = true

[[cli.repair-release.inputs]]
name = "target"
type = "string"
default = "HEAD"

[[cli.repair-release.inputs]]
name = "force"
type = "boolean"
default = "false"

[[cli.repair-release.inputs]]
name = "sync_provider"
type = "boolean"
default = "true"

[[cli.repair-release.inputs]]
name = "format"
type = "choice"
choices = ["text", "json"]
default = "text"

[[cli.repair-release.steps]]
type = "RetargetRelease"
```

<!-- {/cliStepRetargetReleaseExample} -->

<!-- {@cliStepCommandExample} -->

```toml
[cli.test]
help_text = "Run project tests"

[[cli.test.steps]]
type = "Command"
command = "cargo test --workspace --all-features"
dry_run_command = "cargo test --workspace --all-features --no-run"
shell = true
```

<!-- {/cliStepCommandExample} -->

<!-- {@cliStepPrepareReleaseCommandCompositionExample} -->

```toml
[cli.release-with-notes]
help_text = "Prepare a release and print a custom summary"

[[cli.release-with-notes.inputs]]
name = "format"
type = "choice"
choices = ["text", "json"]
default = "text"

[[cli.release-with-notes.steps]]
type = "PrepareRelease"

[[cli.release-with-notes.steps]]
type = "Command"
command = "echo Releasing {{ release.version }} for {{ released_packages }}"
shell = true
```

<!-- {/cliStepPrepareReleaseCommandCompositionExample} -->

<!-- {@cliStepCommandStepOutputExample} -->

```toml
[cli.release-with-generated-notes]
help_text = "Prepare a release, generate notes, and upload them"

[[cli.release-with-generated-notes.steps]]
type = "PrepareRelease"

[[cli.release-with-generated-notes.steps]]
type = "Command"
id = "notes"
command = "printf 'version=%s\n' '{{ release.version }}'"
shell = true

[[cli.release-with-generated-notes.steps]]
type = "Command"
command = "printf '%s\n' '{{ steps.notes.stdout }}'"
shell = true
```

<!-- {/cliStepCommandStepOutputExample} -->

<!-- {@cliStepPlaceholderPublishExample} -->

```toml
[cli.placeholder-publish]
help_text = "Publish placeholder package versions for missing registry packages"

[[cli.placeholder-publish.inputs]]
name = "format"
type = "choice"
choices = ["text", "markdown", "json"]
default = "text"

[[cli.placeholder-publish.inputs]]
name = "package"
type = "string_list"

[[cli.placeholder-publish.steps]]
name = "publish placeholder packages"
type = "PlaceholderPublish"
```

<!-- {/cliStepPlaceholderPublishExample} -->

<!-- {@cliStepPublishPackagesExample} -->

```toml
[cli.publish]
help_text = "Publish package versions from monochange release state using built-in workflows"

[[cli.publish.inputs]]
name = "format"
type = "choice"
choices = ["text", "markdown", "json"]
default = "text"

[[cli.publish.inputs]]
name = "package"
type = "string_list"

[[cli.publish.steps]]
name = "publish packages"
type = "PublishPackages"
```

<!-- {/cliStepPublishPackagesExample} -->

<!-- {@cliStepRetargetCommandCompositionExample} -->

```toml
[cli.repair-and-notify]
help_text = "Repair a release and print the retarget result"

[[cli.repair-and-notify.inputs]]
name = "from"
type = "string"
required = true

[[cli.repair-and-notify.inputs]]
name = "target"
type = "string"
default = "HEAD"

[[cli.repair-and-notify.steps]]
type = "RetargetRelease"

[[cli.repair-and-notify.steps]]
type = "Command"
command = "echo moved {{ retarget.tags }} to {{ retarget.target }} with status {{ retarget.status }}"
shell = true
```

<!-- {/cliStepRetargetCommandCompositionExample} -->
