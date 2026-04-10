---
monochange_config: patch
monochange_core: patch
---

Reject insecure http:// schemes for [source].api_url and [source].host to prevent API tokens from being transmitted in cleartext. Warn when GitHub api_url points to a non-standard host.
