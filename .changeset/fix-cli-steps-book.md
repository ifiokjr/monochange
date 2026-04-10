---
monochange_config: patch
---

#### fix clippy map_unwrap_or lint

Replace `map(<f>).unwrap_or(<a>)` with `map_or(<a>, <f>)` to satisfy the `clippy::map_unwrap_or` lint.
