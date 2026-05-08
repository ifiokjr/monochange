---
monochange_github: minor
monochange: patch
---

# Move GitHub trust context into monochange_github

Move GitHub-specific trust resolution functions out of `monochange::package_publish` and into the provider crate `monochange_github`.

This includes:

- `GitHubTrustContext` and its derive impls
- `resolve_github_trust_context`
- `verify_github_trust_context`
- `trusted_publishing_identity_error`
- `parse_github_workflow_ref`
- `resolve_github_job_environment`
- `trust_list_contains_context`
- `json_value_contains`
- `format_manual_trust_context`
- `GITHUB_ACTIONS_ID_TOKEN_REQUEST_URL` and `GITHUB_ACTIONS_ID_TOKEN_REQUEST_TOKEN`

These functions are now behind the usual `monochange_github` feature flag and can be reused independently of the main publish pipeline.
