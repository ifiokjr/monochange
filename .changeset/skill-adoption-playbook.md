---
"@monochange/skill": minor
---

#### add an adoption playbook and example indexes to the packaged skill

The packaged `@monochange/skill` now includes an interactive adoption guide plus bundled example indexes for choosing how deeply to set up monochange.

**Before:** The skill explained commands, configuration, linting, and publishing, but it did not give agents a clear question tree for quickstart vs standard vs full vs migration adoption. It also lacked a dedicated examples surface for pointing users at setup patterns.

**After:** The package adds `skills/adoption.md`, a bundled `examples/` folder with condensed scenario summaries, and references to a top-level repository `examples/` index for fuller repo-shaped setups.

This makes the skill better at plan-mode interrogation, migration guidance, and recommendation-driven setup conversations.
