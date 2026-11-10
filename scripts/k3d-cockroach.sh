#!/usr/bin/env bash
# Insecure 3-node CockroachDB on a local k3d cluster. Needs: docker, k3d, kubectl, helm.
set -euo pipefail

k3d cluster get crdb >/dev/null 2>&1 || k3d cluster create crdb --agents 3 --wait

helm repo add cockroachdb https://charts.cockroachdb.com/ --force-update
helm upgrade --install crdb cockroachdb/cockroachdb \
  --set tls.enabled=false --set statefulset.replicas=3

kubectl rollout status statefulset/crdb-cockroachdb --timeout=300s
