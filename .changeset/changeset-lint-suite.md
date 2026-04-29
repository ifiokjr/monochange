---
"monochange": minor
"monochange_config": minor
---

# Refactor changeset linting into LintSuite architecture

Move changeset linting out of workspace config loading and into the `LintSuite`/`LintRuleRunner` framework alongside cargo, npm, and dart linting. This ensures `mc check` and `mc validate` surface changeset lint errors consistently.
