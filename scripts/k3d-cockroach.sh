#!/usr/bin/env bash
# Secure (self-signed TLS) 3-node CockroachDB on a local k3d cluster. Needs: docker, k3d, kubectl, helm.
set -euo pipefail

k3d cluster get crdb >/dev/null 2>&1 || k3d cluster create crdb --agents 3 --wait

# tls.enabled=true: the chart generates a self-signed CA + node/client certs by itself
helm repo add cockroachdb https://charts.cockroachdb.com/ --force-update
helm upgrade --install crdb cockroachdb/cockroachdb \
  --set tls.enabled=true --set statefulset.replicas=3

kubectl rollout status statefulset/crdb-cockroachdb --timeout=300s

# Connection handle for local tools: root client certs -> ./certs
mkdir -p certs
kubectl get secret crdb-cockroachdb-client-secret -o jsonpath='{.data.ca\.crt}'  | base64 -d > certs/ca.crt
kubectl get secret crdb-cockroachdb-client-secret -o jsonpath='{.data.tls\.crt}' | base64 -d > certs/client.root.crt
kubectl get secret crdb-cockroachdb-client-secret -o jsonpath='{.data.tls\.key}' | base64 -d > certs/client.root.key
chmod 600 certs/client.root.key

cat <<MSG

Ready. To connect from this machine:
  1) kubectl port-forward svc/crdb-cockroachdb-public 26257:26257   # keep running
  2) export DATABASE_URL="postgresql://root@localhost:26257/defaultdb?sslmode=verify-full&sslrootcert=$PWD/certs/ca.crt&sslcert=$PWD/certs/client.root.crt&sslkey=$PWD/certs/client.root.key"
MSG