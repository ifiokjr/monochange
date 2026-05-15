# deploy monochange_app on DigitalOcean

These are the production deployment steps for `monochange_app`. The initial target is one hardened DigitalOcean Droplet running Docker Compose, Caddy, the Rust/Leptos SSR app, and SQLite on the Droplet disk.

## target shape

- Droplet: Basic 1 GiB RAM minimum, e.g. `s-1vcpu-1gb`.
- Region: choose the closest practical region, e.g. `lon1`.
- Persistent data: host directory at `/opt/monochange/data`.
- Database: SQLite file at `/opt/monochange/data/monochange_app.sqlite3` on the host, mounted into the app as `/data/monochange_app.sqlite3`.
- TLS/reverse proxy: Caddy container with automatic Let's Encrypt certificates.
- Secrets: app loads `secretspec.toml` through the SecretSpec Rust SDK; the container only receives the 1Password service account token as a Docker secret.
- Backups: SQLite `.backup` snapshots plus off-Droplet copy to object storage.

## why no DigitalOcean block volume initially

DigitalOcean Block Storage Volumes can only be attached to one Droplet at a time for this use case. They are useful when you want to detach data from one Droplet and attach it to a replacement Droplet in the same region, or resize/snapshot that disk independently.

They do **not** make SQLite active/active, do **not** reduce deployment downtime by themselves, and do **not** allow two app Droplets to safely write the same SQLite database. Since our initial plan is one Droplet with offsite backups, the extra volume lifecycle is not worth the complexity yet.

If we later need lower downtime or multiple app instances, the right move is not a shared block volume. The right move is either:

- same-Droplet blue/green app containers with one SQLite writer; or
- moving the database to PostgreSQL, Turso/libSQL, or another networked database before multi-Droplet active/active.

## automation and hardening approach

Use two layers:

1. `doctl` for the first documented deployment path.
2. Later, codify the same shape with Pulumi/OpenTofu plus cloud-init or Ansible.

Recommended hardening baseline:

- disable password SSH login;
- use SSH keys only;
- create a non-root deploy user;
- install Docker from the official apt repository;
- enable UFW and allow only `22`, `80`, and `443`;
- install and enable `fail2ban`;
- enable unattended security upgrades;
- keep `/opt/monochange/secrets/op_service_account_token` mode `0600`;
- keep the 1Password service account scoped to the minimal production vault/items;
- run the app container as the non-root `app` user baked into the image;
- use off-Droplet backups.

## prerequisites

- `doctl` authenticated locally:

```bash
brew install doctl
doctl auth init
```

- a domain pointed at the Droplet IPv4 address, for example `app.monochange.dev`;
- a GitHub OAuth app with callback URL:

```text
https://app.monochange.dev/api/oauth/callback
```

- a 1Password service account scoped to the monochange production secrets;
- production SecretSpec values stored in 1Password for project `monochange_app`, profile `production`:
  - `DATABASE_URL=sqlite:///data/monochange_app.sqlite3`
  - `JWT_SECRET`
  - `GITHUB_CLIENT_ID`
  - `GITHUB_CLIENT_SECRET`
  - optional `GITHUB_APP_ID`, `GITHUB_APP_PRIVATE_KEY`, `OPENROUTER_API_KEY`.

## 1. create the Droplet

```bash
REGION=lon1
DROPLET_NAME=monochange-app
SSH_KEY_ID="$(doctl compute ssh-key list --format ID --no-header | head -n1)"

doctl compute droplet create "$DROPLET_NAME" \
  --region "$REGION" \
  --image ubuntu-24-04-x64 \
  --size s-1vcpu-1gb \
  --ssh-keys "$SSH_KEY_ID" \
  --wait

DROPLET_IP="$(doctl compute droplet get "$DROPLET_NAME" --format PublicIPv4 --no-header)"
echo "$DROPLET_IP"
```

Point DNS for `app.monochange.dev` at `DROPLET_IP` before starting Caddy.

## 2. create the DigitalOcean firewall

Create a cloud firewall before starting the app. By default this allows SSH, HTTP, and HTTPS. If your SSH source IP is stable, set `SSH_SOURCES` to that CIDR before running the script.

```bash
# Optional, more secure if your IP is stable:
# export SSH_SOURCES="$(curl -fsS https://ifconfig.me)/32"

DROPLET_NAME=$DROPLET_NAME \
  apps/monochange_app/deploy/digitalocean/create-firewall.sh
```

Cloud firewall rules:

- inbound `22/tcp` from `SSH_SOURCES` (defaults to the internet for portability);
- inbound `80/tcp` and `443/tcp` from the internet;
- outbound TCP/UDP to the internet for package installs, registry pulls, 1Password, GitHub, and ACME.

## 3. bootstrap and harden the server

Run the hardened setup script on the Droplet. It installs Docker, configures SSH key-only access, creates the `deploy` user, enables UFW/fail2ban/unattended upgrades, creates app directories, and installs backup/deploy helper scripts.

```bash
scp apps/monochange_app/deploy/digitalocean/harden-droplet.sh root@$DROPLET_IP:/root/harden-droplet.sh
ssh root@$DROPLET_IP 'chmod +x /root/harden-droplet.sh && /root/harden-droplet.sh'
```

The script enforces:

- `PasswordAuthentication no`;
- `KbdInteractiveAuthentication no`;
- `ChallengeResponseAuthentication no`;
- `PubkeyAuthentication yes`;
- `PermitRootLogin prohibit-password`;
- `AllowUsers deploy root` so `deploy` is used for normal deploys while root remains key-only for privileged maintenance;
- UFW default-deny incoming, allow `22`, `80`, `443`;
- fail2ban SSH jail;
- unattended security upgrades;
- `/opt/monochange/secrets` mode `0700`;
- `/opt/monochange/secrets/op_service_account_token` should be written with mode `0600`.

After this step, use `deploy@$DROPLET_IP` for ordinary deployment and `root@$DROPLET_IP` only for privileged maintenance.

## 4. copy deploy files and bootstrap the 1Password token

```bash
scp apps/monochange_app/deploy/digitalocean/docker-compose.yml deploy@$DROPLET_IP:/tmp/docker-compose.yml
scp apps/monochange_app/deploy/digitalocean/Caddyfile deploy@$DROPLET_IP:/tmp/Caddyfile
ssh root@$DROPLET_IP 'mv /tmp/docker-compose.yml /opt/monochange/docker-compose.yml && mv /tmp/Caddyfile /opt/monochange/Caddyfile'
```

Store the 1Password service account token as a Docker secret source file on the host:

```bash
ssh root@$DROPLET_IP 'install -m 600 /dev/stdin /opt/monochange/secrets/op_service_account_token' <<'EOF'
ops_...
EOF
```

Runtime flow:

1. Compose mounts `/opt/monochange/secrets/op_service_account_token` as `/run/secrets/onepassword_service_account_token`.
2. The entrypoint exports it as `OP_SERVICE_ACCOUNT_TOKEN`.
3. `monochange_app` loads `secretspec.toml` through the SecretSpec SDK.
4. SecretSpec invokes the bundled `op` CLI and reads production secrets from 1Password.
5. The typed secret set is stored in `AppState` for server handlers.

## 5. build and upload the Docker image

Manual first deploy from the repo root:

```bash
docker build -t monochange-app:latest .
docker save monochange-app:latest | gzip | ssh root@$DROPLET_IP 'gunzip | docker load'
```

Later, CI should build the image, push it to GHCR or DigitalOcean Container Registry, then SSH to the Droplet and run `docker compose pull app && docker compose up -d`.

## 6. start the app

```bash
ssh root@$DROPLET_IP 'cd /opt/monochange && docker compose up -d'
```

## 7. verify

```bash
curl -fsS https://app.monochange.dev/health | jq
ssh root@$DROPLET_IP 'cd /opt/monochange && docker compose ps && docker compose logs --tail=100 app'
```

Expected health response:

```json
{
	"status": "ok",
	"http": "up"
}
```

## 8. SQLite backups

Droplet backups are useful, but they are not enough. Keep SQLite-consistent backups and copy them off-Droplet.

Install a local backup script:

```bash
ssh root@$DROPLET_IP 'cat >/usr/local/bin/monochange-sqlite-backup <<EOF
#!/usr/bin/env bash
set -euo pipefail
DB=/opt/monochange/data/monochange_app.sqlite3
BACKUP_DIR=/opt/monochange/backups
STAMP=\$(date -u +%Y%m%dT%H%M%SZ)
mkdir -p "\$BACKUP_DIR"
sqlite3 "\$DB" ".backup \$BACKUP_DIR/monochange_app-\$STAMP.sqlite3"
find "\$BACKUP_DIR" -name "monochange_app-*.sqlite3" -mtime +14 -delete
EOF
chmod +x /usr/local/bin/monochange-sqlite-backup
cat >/etc/cron.daily/monochange-sqlite-backup <<EOF
#!/usr/bin/env bash
set -euo pipefail
/usr/local/bin/monochange-sqlite-backup
EOF
chmod +x /etc/cron.daily/monochange-sqlite-backup'
```

Add offsite sync next. Preferred first option: `rclone` to Backblaze B2 or DigitalOcean Spaces. Later, use `restic` if encrypted deduplicated backups become important.

## GitHub Actions deploy access

Use separate SSH keys for local access and CI deploys. Do not put a personal laptop private key in GitHub Actions.

Recommended setup:

- local key: your normal SSH key or a dedicated `monochange_do` key;
- CI key: a separate `github-actions-monochange-deploy` ed25519 key;
- both public keys are added to `/home/deploy/.ssh/authorized_keys`;
- GitHub Actions stores only the CI private key in a production environment secret.

Create the CI key locally:

```bash
ssh-keygen -t ed25519 -C "github-actions-monochange-deploy" -f ./monochange_actions_deploy
cat ./monochange_actions_deploy.pub | ssh root@$DROPLET_IP 'cat >>/home/deploy/.ssh/authorized_keys'
rm ./monochange_actions_deploy.pub
```

Store `./monochange_actions_deploy` as a GitHub Actions environment secret named `DO_SSH_PRIVATE_KEY`, then delete the local private key copy after storing it. Also store:

- `DO_HOST`: Droplet IP or hostname;
- `DO_USER`: `deploy`.

For stronger CI lockdown, prefix the GitHub Actions public key in `authorized_keys` with a forced command so that key can only run the deploy script:

```text
command="/usr/local/bin/monochange-deploy",no-agent-forwarding,no-X11-forwarding,no-pty ssh-ed25519 AAAA... github-actions-monochange-deploy
```

With that restriction, GitHub Actions can trigger deployment but cannot open an arbitrary shell with that key. Local deploy keys should stay unrestricted for maintenance.

DigitalOcean's API can manage infrastructure, firewalls, images, and Droplets, but it is not a general remote-command API for a raw Droplet. For `docker compose up -d`, SSH or a small deploy agent is still required. SSH with a dedicated deploy key and optional forced command is the simplest secure path.

## update deploy

```bash
docker build -t monochange-app:latest .
docker save monochange-app:latest | gzip | ssh root@$DROPLET_IP 'gunzip | docker load'
ssh root@$DROPLET_IP 'cd /opt/monochange && docker compose up -d'
curl -fsS https://app.monochange.dev/health | jq
```

Expected downtime for this initial deploy shape is small, usually one app restart window. Static assets may remain cached by Cloudflare/Caddy, but SSR/API requests can fail during the restart.

## path to lower downtime

A separate block volume is not the path to lower downtime for SQLite. Safe next steps:

1. Add image registry deploys and health-gated restart.
2. Add Caddy with two local app upstreams for same-Droplet blue/green.
3. Keep only one writer active during migration windows.
4. If we need multi-Droplet or true zero-downtime writes, move from SQLite to PostgreSQL, Turso/libSQL, or another networked DB.

Do not mount the same SQLite database as an active writable database across multiple Droplets.
