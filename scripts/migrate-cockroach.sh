#!/usr/bin/env bash
# Validate the DB from OUTSIDE the cluster, using DATABASE_URL (certs are read from the host paths in the URL).
# Prerequisites (not handled here):
#   - kubectl port-forward svc/crdb-cockroachdb-public 26257:26257
#   - DATABASE_URL exported (printed at the end of scripts/k3d-cockroach.sh)
set -euo pipefail

SCRIPTDIR=$( cd -- "$( dirname -- "${BASH_SOURCE[0]}" )" &> /dev/null && pwd )

: "${DATABASE_URL:?DATABASE_URL is not set, see the end of the scripts/k3d-cockroach.sh output}"

# Strip the username (e.g., root@) from DATABASE_URL for JDBC/Flyway compatibility,
# as pgjdbc does not support user@host in the URI authority section.
JDBC_URL="jdbc:$(echo "$DATABASE_URL" | sed -E 's|^(postgresql://)[^@]*@|\1|')"

flyway \
  "-url=${JDBC_URL}" \
  -user=root \
  "-locations=filesystem:${SCRIPTDIR}/../migrations" \
  -executeInTransaction=false \
  migrate