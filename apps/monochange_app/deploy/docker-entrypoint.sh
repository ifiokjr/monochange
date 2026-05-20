#!/usr/bin/env sh
set -eu

if [ -n "${OP_SERVICE_ACCOUNT_TOKEN_FILE:-}" ] && [ -f "${OP_SERVICE_ACCOUNT_TOKEN_FILE}" ]; then
	OP_SERVICE_ACCOUNT_TOKEN="$(cat "${OP_SERVICE_ACCOUNT_TOKEN_FILE}")"
	export OP_SERVICE_ACCOUNT_TOKEN
fi

exec "$@"
