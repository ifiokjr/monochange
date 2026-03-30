---
monochange: patch
monochange_core: patch
monochange_config: patch
monochange_cargo: patch
monochange_npm: patch
monochange_deno: patch
monochange_dart: patch
---

#### harden package identity rendering, diagnostics, and CLI verification

Normalize package id rendering and repository-relative path output so discovery, validation, planning, and release workflow output stays stable across relative and absolute invocation roots. This update also improves validation diagnostics, expands `insta-cmd` CLI coverage for validation, discovery, change creation, and release flows, adds cross-ecosystem release workflow integration coverage, and syncs the documentation to describe deterministic relative-path output.
