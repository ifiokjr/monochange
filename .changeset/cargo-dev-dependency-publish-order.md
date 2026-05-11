---
monochange: patch
monochange_publish: patch
---

# Include Cargo development dependencies in publish ordering

Cargo package publishing now orders runtime, build, and development dependencies before dependents. This prevents a crate from being published before an unpublished workspace crate referenced through `dev-dependencies` or `build-dependencies`.
