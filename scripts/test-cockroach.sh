#!/usr/bin/env bash
# Validate the DB from OUTSIDE the cluster, through localhost:26257.
# Prerequisite (not handled here): kubectl port-forward svc/crdb-cockroachdb-public 26257:26257
# Needs docker + kubectl; the cockroach client runs in a container, no local SQL client.
set -euo pipefail
cd "$(dirname "$0")/.."

(exec 3<>/dev/tcp/127.0.0.1/26257) 2>/dev/null \
  || { echo "nothing listening on localhost:26257, start the port-forward first" >&2; exit 1; }

IMAGE=$(kubectl get statefulset crdb-cockroachdb -o jsonpath='{.spec.template.spec.containers[0].image}')

# Linux: share the host network. Docker Desktop (Mac/Windows): reach the host via host.docker.internal.
if [[ "$(uname)" == Linux ]]; then
  NET=(--network host); HOST=localhost; MODE=verify-full
else
  NET=(--add-host=host.docker.internal:host-gateway); HOST=host.docker.internal; MODE=verify-ca
fi
URL="postgresql://root@${HOST}:26257/defaultdb?sslmode=${MODE}&sslrootcert=/certs/ca.crt&sslcert=/certs/client.root.crt&sslkey=/certs/client.root.key"

# runs the cockroach CLI in a container: crdb <subcommand> [args]
crdb() {
  docker run --rm "${NET[@]}" -e COCKROACH_SKIP_KEY_PERMISSION_CHECK=true \
    -v "$PWD/certs:/certs:ro" "$IMAGE" "$@" --url "$URL"
}
trap 'crdb sql -e "DROP TABLE IF EXISTS smoke" >/dev/null 2>&1 || true' EXIT

crdb node status
crdb sql -e "CREATE TABLE smoke (i INT); INSERT INTO smoke VALUES (1); SELECT * FROM smoke; DROP TABLE smoke"