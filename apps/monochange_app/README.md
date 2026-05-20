# monochange_app

Leptos SSR web app for monochange release planning.

## local development

The app uses SQLite by default, so no database service is required.

```bash
# From the repository root
devenv shell cargo leptos --manifest-path apps/monochange_app/crates/monochange_app/Cargo.toml serve
```

Default database:

```text
sqlite://.devenv/state/monochange_app.sqlite3
```

Override with `DATABASE_URL` when needed:

```bash
DATABASE_URL=sqlite://./monochange_app.sqlite3 devenv shell cargo leptos --manifest-path apps/monochange_app/crates/monochange_app/Cargo.toml serve
```

## tests

```bash
devenv shell cargo test --manifest-path apps/monochange_app/Cargo.toml -p monochange_app_db --lib
devenv shell cargo check --manifest-path apps/monochange_app/Cargo.toml -p monochange_app
```

Release automation uses the same SQLite database and remains disabled unless `MONOCHANGE_APP_AUTOMATION` is explicitly enabled.

## deploy

See [DEPLOY.md](./DEPLOY.md) for the DigitalOcean Docker deployment guide.
