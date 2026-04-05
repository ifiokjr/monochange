---
monochange: minor
---

#### add configurable source providers for release automation

MonoChange now supports provider-driven repository automation through `[source]` configuration. GitHub remains the default provider, while GitLab and Gitea can now drive release publication and release-request previews through dedicated integration crates. The source automation steps are now provider-neutral: use `PublishRelease` and `OpenReleaseRequest`, while legacy GitHub-specific step names continue to work as migration aliases.
