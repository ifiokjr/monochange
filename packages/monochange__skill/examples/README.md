# monochange skill examples

Use these condensed examples when the main skill needs quick setup guidance without loading the larger repo-shaped examples from the monochange repository.

## Example index

- [quickstart.md](./quickstart.md) — choosing between quickstart and standard adoption
- [migration.md](./migration.md) — adopting monochange into an existing repository safely
- [publishing.md](./publishing.md) — builtin vs external publishing, trusted publishing, and placeholders
- [release-pr.md](./release-pr.md) — long-running release PR branch recommendations

## How to use this folder

- start with `skills/adoption.md` when the user is still choosing setup depth
- open one of the example pages here when you need a short recommendation pattern
- use the top-level repository examples for fuller CI shapes and future end-to-end fixtures

Full repository examples live in the monochange repository at:

- <https://github.com/ifiokjr/monochange/tree/main/examples>

That folder also includes `examples/validate-examples.sh` for running `mc validate`, `mc check`, and `mc release --dry-run --diff` across the repo-shaped examples.
