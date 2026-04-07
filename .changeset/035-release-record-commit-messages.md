---
monochange: minor
monochange_core: minor
monochange_github: minor
monochange_gitlab: minor
monochange_gitea: minor
---

#### add structured release commit messages with embedded ReleaseRecord blocks

MonoChange-managed release-request commits now carry a generated commit body instead of only a subject line.

Before, provider release-request payloads only carried a string commit message subject:

```rust
commit_message: "chore(release): prepare release".to_string()
```

After, they carry a structured commit message with a subject and optional body:

```rust
commit_message: CommitMessage {
    subject: "chore(release): prepare release".to_string(),
    body: Some("...generated release summary and ReleaseRecord block...".to_string()),
}
```

For `mc release-pr` and equivalent provider flows, the generated commit body now includes:

- a compact release summary for humans
- the reserved `## MonoChange Release Record` fenced JSON block for durable release history

This is the first step toward repairable releases built from commit history rather than repository receipt files.
