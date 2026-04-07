---
monochange: minor
monochange_config: minor
monochange_core: minor
monochange_github: patch
monochange_gitlab: patch
monochange_gitea: patch
---

Add the `CommitRelease` CLI step and the default `mc commit-release` command for creating a local release commit with an embedded MonoChange release record.

This also lets provider release-request publishing reuse an already-created local release commit without failing when there is nothing new to commit on the release branch.
