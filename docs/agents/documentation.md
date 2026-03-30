# Documentation workflow

Shared documentation blocks live in `.templates/` and are synchronized with `mdt`.

- Edit provider blocks in `.templates/` when one change should update multiple docs.
- Run `docs:update` after changing shared docs or consumer blocks.
- Run `docs:check` before opening a PR to confirm shared blocks are synchronized.
- Treat `docs/` as a product surface when behavior changes.
