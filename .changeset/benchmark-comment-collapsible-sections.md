---
monochange: feat
---

#### add visual status summary to benchmark CI comment sections

`monochange` benchmark PR comments now show an at-a-glance status summary inside each collapsed `<details>` section, so reviewers can see improvements and regressions without expanding anything.

**Before:**

- benchmark PR comments rendered every fixture table and phase timing table fully expanded
- scrolling to later fixtures required paging through the entire earlier benchmark output
- when sections were collapsed, there was no way to tell if a fixture improved or regressed without expanding it

**After:**

- each benchmark fixture renders as a collapsed section with a summary line showing emoji indicators
- per-command status: 🟢 improved · 🔴 regressed · ⚪ flat (for hyperfine tables with relative data)
- phase-level status: 🟢 phases improved · 🔴 phases regressed (for tables without relative comparison data)
- 🚨 over budget shown when any phase exceeds its configured budget
- reviewers can expand only the fixture tables they need while keeping the rest of the comment compact
