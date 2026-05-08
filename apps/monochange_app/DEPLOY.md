# deploy monochange_app

These instructions get the `monochange_app` Leptos SSR site running on Fly.io in minutes.

## prerequisites

- [flyctl](https://fly.io/docs/hands-on/install-flyctl/) installed and authenticated
- a registered [GitHub OAuth app](https://github.com/settings/developers) with a client ID and secret
- the monochange_app **database migrations** run on a live Postgres instance (see step 2)

## setup

### 1. create the fly app

```bash
fly apps create --name monochange-app
```

### 2. create a postgres cluster and database

```bash
fly postgres create --name monochange-app-db
```

Attach it to the app and grab the connection string:

```bash
fly postgres attach --app monochange-app --database-name monochange_app monochange-app-db
```

Run migrations from the repo root:

```bash
DATABASE_URL="$FLY_DATABASE_URL" \
  devenv shell cargo run \
  --manifest-path apps/monochange_app/crates/monochange_app_db/Cargo.toml \
  --bin migrate
```

### 3. set secrets

```bash
fly secrets set \
  DATABASE_URL="<postgres-connection-string>" \
  JWT_SECRET="<generate-a-random-64-byte-hex-string>" \
  GITHUB_CLIENT_ID="<your-oauth-app-client-id>" \
  GITHUB_CLIENT_SECRET="<your-oauth-app-client-secret>" \
  LEPTOS_SITE_ADDR="0.0.0.0:3000" \
  LEPTOS_SITE_PKG_DIR="pkg"
```

The app reads `$PORT` at runtime as a fallback for `LEPTOS_SITE_ADDR` (Fly injects this automatically), but pinning `LEPTOS_SITE_ADDR` is recommended for clarity.

### 4. update github oauth callback URL

In your GitHub OAuth App settings, change the **Authorization callback URL** to:

```
https://monochange-app.fly.dev/api/oauth/callback
```

Replace the hostname with your custom domain if you added one.

### 5. deploy

```bash
fly deploy
```

The multi-stage `Dockerfile` at the repo root builds the WASM frontend and the Rust server binary, then copies both into a slim Debian final stage. The binary listens on port `3000` by default and exposes a `GET /health` endpoint used by Fly's service check.

### 6. verify

```bash
fly status
curl -s https://monochange-app.fly.dev/health | jq
```

Expected:

```json
{
	"status": "ok",
	"http": "up"
}
```

## optional: enable the automation worker

The background release-automation worker is **disabled by default**. To start it, set an additional secret and redeploy:

```bash
fly secrets set MONOCHANGE_APP_AUTOMATION=true
fly deploy
```

In the current build only `AutomationRuntimeMode::DryRun` is wired up, so no real repository actions are dispatched.

## useful fly commands

| command             | purpose                                 |
| ------------------- | --------------------------------------- |
| `fly logs`          | tail application logs                   |
| `fly ssh console`   | drop into the running container         |
| `fly secrets list`  | inspect configured secrets (names only) |
| `fly scale count 2` | scale to two machines for HA            |

## build locally

```bash
# dev build
devenv shell cargo leptos --manifest-path apps/monochange_app/crates/monochange_app/Cargo.toml build

# release build
devenv shell cargo leptos --manifest-path apps/monochange_app/crates/monochange_app/Cargo.toml build --release
```
