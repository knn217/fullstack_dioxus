#!/usr/bin/env bash
# Remove everything created by k3d-cockroach.sh (pods, volumes/data, kubeconfig context).
set -euo pipefail

k3d cluster delete crdb
helm repo remove cockroachdb 2>/dev/null || true