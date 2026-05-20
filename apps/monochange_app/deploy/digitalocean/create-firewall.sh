#!/usr/bin/env bash
set -euo pipefail

DROPLET_NAME="${DROPLET_NAME:-monochange-app}"
FIREWALL_NAME="${FIREWALL_NAME:-monochange-app-firewall}"
SSH_SOURCES="${SSH_SOURCES:-0.0.0.0/0,::/0}"

DROPLET_ID="$(doctl compute droplet get "${DROPLET_NAME}" --format ID --no-header)"

if doctl compute firewall get "${FIREWALL_NAME}" >/dev/null 2>&1; then
	doctl compute firewall add-droplets "${FIREWALL_NAME}" --droplet-ids "${DROPLET_ID}"
else
	doctl compute firewall create \
		--name "${FIREWALL_NAME}" \
		--inbound-rules "protocol:tcp,ports:22,address:${SSH_SOURCES} protocol:tcp,ports:80,address:0.0.0.0/0,address:::/0 protocol:tcp,ports:443,address:0.0.0.0/0,address:::/0" \
		--outbound-rules "protocol:tcp,ports:all,address:0.0.0.0/0,address:::/0 protocol:udp,ports:all,address:0.0.0.0/0,address:::/0" \
		--droplet-ids "${DROPLET_ID}"
fi
