---
monochange: major
---

#### replace built-in command structure with workflow-defined top-level commands

- move the CLI surface to top-level workflow commands such as `mc validate`, `mc discover`, `mc change`, and `mc release`
- synthesize default workflows when `monochange.toml` omits them and add `mc init` to write explicit starter config
- rename the validation workflow step from `Check` to `Validate` and remove the old nested command model
