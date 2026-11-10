#!/usr/bin/env bash
set -euo pipefail

kubectl exec crdb-cockroachdb-0 -- /cockroach/cockroach node status --insecure
kubectl exec crdb-cockroachdb-0 -- /cockroach/cockroach sql --insecure \
  -e "CREATE TABLE smoke (i INT); INSERT INTO smoke VALUES (1); SELECT * FROM smoke; DROP TABLE smoke;"
