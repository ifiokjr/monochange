# Quickstart example

## Recommend this when

- the repository is greenfield or nearly greenfield
- the user wants a safe first success before full automation
- package discovery and config quality matter more than publish setup right now

## Default recommendation

- ask for setup depth first: `quickstart` or `standard`
- inspect the repository before asking ecosystem-specific questions
- start with `mc init`, `mc validate`, `mc discover --format json`, and `mc release --dry-run --diff`
- stop before real publishing unless the user explicitly wants a deeper setup

## Good default output

- detected provider and ecosystems
- recommended lint profile
- whether grouping is needed yet
- next commands to run
- what to defer until `full` mode
