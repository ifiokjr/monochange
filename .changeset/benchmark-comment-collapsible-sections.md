---
monochange: patch
---

#### collapse benchmark CI comment sections for faster review

`monochange` now wraps each benchmark fixture section in a GitHub `<details>` block so large benchmark comments are easier to scan.

**Before:**

- benchmark PR comments rendered every fixture table and phase timing table fully expanded
- scrolling to later fixtures required paging through the entire earlier benchmark output

**After:**

- each benchmark fixture renders as a collapsed section with a summary line
- reviewers can expand only the fixture tables they need while keeping the rest of the comment compact
