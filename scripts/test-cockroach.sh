#!/usr/bin/env bash
# Validate the DB from OUTSIDE the cluster, using DATABASE_URL (certs are read from the host paths in the URL).
# Prerequisites (not handled here):
#   - kubectl port-forward svc/crdb-cockroachdb-public 26257:26257
#   - DATABASE_URL exported (printed at the end of scripts/k3d-cockroach.sh)
#   - psql installed (preinstalled on GitHub's ubuntu runners)
set -euo pipefail

: "${DATABASE_URL:?DATABASE_URL is not set, see the end of the scripts/k3d-cockroach.sh output}"

command -v psql >/dev/null \
  || { echo "psql not found (apt install postgresql-client / brew install libpq)" >&2; exit 1; }

(exec 3<>/dev/tcp/127.0.0.1/26257) 2>/dev/null \
  || { echo "nothing listening on localhost:26257, start the port-forward first" >&2; exit 1; }

trap 'psql "$DATABASE_URL" -X -q -c "DROP TABLE IF EXISTS smoke" >/dev/null 2>&1 || true' EXIT

psql "$DATABASE_URL" -X -v ON_ERROR_STOP=1 -c "SELECT version()"
psql "$DATABASE_URL" -X -v ON_ERROR_STOP=1 -f - <<'SQL'
CREATE TABLE smoke (i INT);
INSERT INTO smoke VALUES (1);
SELECT * FROM smoke;
DROP TABLE smoke;
SQL