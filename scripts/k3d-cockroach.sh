#!/usr/bin/env bash
# Secure (self-signed TLS) 3-node CockroachDB on a local k3d cluster. Needs: docker, k3d, kubectl, helm.
set -euo pipefail

# Publish 26257 on the host through k3d's load balancer.
# NOTE: port mappings are fixed at cluster creation; an existing "crdb" cluster
# created without this flag must be deleted first (k3d cluster delete crdb).
k3d cluster get crdb >/dev/null 2>&1 || k3d cluster create crdb --agents 3 \
  -p "26257:26257@loadbalancer" --wait

# tls.enabled=true: the chart generates a self-signed CA + node/client certs by itself
# service.public.type=LoadBalancer: k3s binds 26257 on the nodes, k3d forwards the host port to it
helm repo add cockroachdb https://charts.cockroachdb.com/ --force-update
helm upgrade --install crdb cockroachdb/cockroachdb \
  --set tls.enabled=true \
  --set statefulset.replicas=3 \
  --set service.public.type=LoadBalancer

kubectl rollout status statefulset/crdb-cockroachdb --timeout=300s

# Connection handle for local tools: root client certs -> ./certs
mkdir -p certs
kubectl get secret crdb-cockroachdb-client-secret -o jsonpath='{.data.ca\.crt}'  | base64 -d > certs/ca.crt
kubectl get secret crdb-cockroachdb-client-secret -o jsonpath='{.data.tls\.crt}' | base64 -d > certs/client.root.crt
kubectl get secret crdb-cockroachdb-client-secret -o jsonpath='{.data.tls\.key}' | base64 -d > certs/client.root.key
chmod 600 certs/client.root.key

cat <<MSG

Ready. Port 26257 is published on localhost by the k3d load balancer (no port-forward needed).
To connect from this machine:
  export DATABASE_URL="postgresql://root@localhost:26257/defaultdb?sslmode=verify-full&sslrootcert=$PWD/certs/ca.crt&sslcert=$PWD/certs/client.root.crt&sslkey=$PWD/certs/client.root.key"
MSG