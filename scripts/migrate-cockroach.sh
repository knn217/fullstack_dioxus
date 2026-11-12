#!/usr/bin/env bash
# Validate the DB from OUTSIDE the cluster, using DATABASE_URL (certs are read from the host paths in the URL).
# Prerequisites (not handled here):
#   - kubectl port-forward svc/crdb-cockroachdb-public 26257:26257
#   - DATABASE_URL exported (printed at the end of scripts/k3d-cockroach.sh)
#   - psql installed (preinstalled on GitHub's ubuntu runners)
set -euo pipefail

SCRIPTDIR=$( cd -- "$( dirname -- "${BASH_SOURCE[0]}" )" &> /dev/null && pwd )

: "${DATABASE_URL:?DATABASE_URL is not set, see the end of the scripts/k3d-cockroach.sh output}"

flyway \
  "-url=jdbc:${DATABASE_URL}" \
  -user=root \
  "-locations=filesystem:${SCRIPTDIR}/../migrations" \
  -executeInTransaction=false \
  migrate