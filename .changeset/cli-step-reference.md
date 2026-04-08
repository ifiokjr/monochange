---
monochange_core: patch
---

#### add a built-in CLI step reference for workflow authors

monochange now documents every built-in `[[cli.<command>.steps]]` step in a dedicated reference section, with guidance on when to use each step, what prerequisite state it expects, and how to compose it with later workflow steps.

**Before:** workflow authors had to infer newer step behavior such as `CommitRelease`, `DiagnoseChangesets`, and `RetargetRelease` from scattered examples or by reading the enum definitions directly.

```rust
use monochange_core::CliStepDefinition;

let step = CliStepDefinition::RetargetRelease {
    inputs: Default::default(),
};
```

**After:** the book includes a per-step reference with detailed examples, and `monochange_core::CliStepDefinition` now carries clearer API docs for each built-in step.

```toml
[cli.repair-release]
help_text = "Repair a recent release by retargeting its tags"

[[cli.repair-release.inputs]]
name = "from"
type = "string"
required = true

[[cli.repair-release.steps]]
type = "RetargetRelease"
```

This makes it easier to discover the difference between standalone inspection or repair steps (`Validate`, `Discover`, `AffectedPackages`, `DiagnoseChangesets`, `RetargetRelease`) and release-state consumer steps that require `PrepareRelease` first (`CommitRelease`, `RenderReleaseManifest`, `PublishRelease`, `OpenReleaseRequest`, `CommentReleasedIssues`).
