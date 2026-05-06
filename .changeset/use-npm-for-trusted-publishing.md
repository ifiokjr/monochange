---
main: patch
---

# Use npm for trusted npm publishing

Route trusted npm publishes through the npm CLI even in pnpm-managed workspaces so npm's OIDC trusted publishing flow can exchange the GitHub Actions identity for a short-lived publish credential. The release workflow also relies on devenv environment cleaning directly instead of the removed `strip:env` wrapper.
