---
monochange: patch
monochange_cargo: patch
monochange_publish: patch
---

# Validate Cargo private dependency publishing hazards

Cargo linting now reports publishable packages that depend on private workspace packages through `dependencies`, `dev-dependencies`, or `build-dependencies`. Package publish dry runs now execute the registry dry-run command and preserve its stdout and stderr in the publish report instead of only planning the command.
