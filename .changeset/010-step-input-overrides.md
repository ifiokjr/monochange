---
monochange: patch
monochange_config: patch
monochange_core: patch
---

#### add CLI step input overrides and namespaced template inputs

CLI steps can now override the inputs they receive with an `inputs = { ... }` map while still defaulting to the command's declared inputs. Command templates also expose CLI inputs through `{{ inputs.name }}` so workflows can rebind values explicitly without colliding with built-in template variables.
