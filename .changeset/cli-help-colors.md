---
"monochange": patch
---

Fix `--help` (`-h`) color output and unify CLI color palette.

- `mc --help` now emits ANSI colors in terminal emulators, matching `mc help <command>` behavior
- Extract shared `cli_theme` module so clap built-in help and custom `mc help` renderer use identical colors:
  - bright cyan for headers and accents
  - bright white for usage
  - bright yellow for flags and literals
  - bright magenta for placeholders
  - bright green for valid/code snippets
  - bright red for errors
  - bright black (gray) for muted text
- Explicitly opt in to `ColorChoice::Auto` on the `Command` builder
- Preserve plain text output in test and CI modes so existing snapshots stay stable
