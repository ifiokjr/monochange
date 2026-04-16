---
monochange: minor
monochange_dart: minor
monochange_core: none
monochange_config: none
monochange_cargo: none
monochange_npm: none
monochange_lint: none
monochange_deno: none
monochange_gitea: none
monochange_gitlab: none
"@monochange/cli": none
"@monochange/skill": none
---

#### add the first Dart lint suite foundation

monochange now wires Dart manifests into the ecosystem-owned lint registry and supports scaffolding Dart lint files with `mc lint new dart/<rule-name>`.

This foundation change adds:

- a new `monochange_dart::lints` module with target collection for `pubspec.yaml`
- Dart lint suite registration in the `mc lint` and `mc check` command paths
- Dart lint scaffolding support in `mc lint new`
- tests covering managed Dart lint target collection and fixture filtering
