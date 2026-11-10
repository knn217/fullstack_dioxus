#!/usr/bin/env bash
# Remove everything created by k3d-cockroach.sh (cluster, data, local certs, helm repo).
set -euo pipefail

k3d cluster delete crdb
rm -rf certs
helm repo remove cockroachdb 2>/dev/null || true