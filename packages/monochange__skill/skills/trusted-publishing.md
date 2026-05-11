# Trusted publishing

monochange publishing settings can opt packages into trusted/OIDC publishing where supported.

```toml
[ecosystems.npm.publish]
trusted_publishing = true

[package."@acme/api".publish]
enabled = true
mode = "builtin"
registry = "npm"
trusted_publishing = true
```

Use a table when the trust context needs explicit repository/workflow/environment metadata:

```toml
[package."@acme/api".publish.trusted_publishing]
enabled = true
repository = "acme/widgets"
workflow = "publish.yml"
environment = "npm"
```

Registry support and setup requirements vary. Treat trusted-publishing setup as a registry-side operation that may require a human maintainer. Agents should generate configuration and workflow code, but should not use local credentials or perform registry-side changes unless explicitly authorized and allowed by project policy.
