#!/bin/sh
# Writes WIFI_SSID and WIFI_PASSWORD from .env to the device's config partition
# (see partitions.csv). Run once per device; firmware updates never touch it.
set -eu
cd "$(dirname "$0")/.."
set -a
. ./.env
set +a

config=$(mktemp)
trap 'rm -f "$config"' EXIT
printf '%s\n%s\n' "$WIFI_SSID" "$WIFI_PASSWORD" > "$config"
espflash write-bin --chip esp32 --non-interactive 0x3F0000 "$config" "$@"
