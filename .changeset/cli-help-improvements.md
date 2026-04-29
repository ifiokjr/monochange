---
monochange: minor
---

# Add colored CLI help

Add beautiful colored CLI help with detailed examples

The `mc help <command>` subcommand now renders detailed, formatted help with bordered headers, colored sections, multiple examples per command, tips, and cross-references. Running `mc help` shows an overview listing all commands. The standard `--help` flags also use ANSI colors via an anstyle theme. All colors respect NO_COLOR and TTY detection.
