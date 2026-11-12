#!/usr/bin/env bash
set -euo pipefail

SCRIPTDIR=$( cd -- "$( dirname -- "${BASH_SOURCE[0]}" )" &> /dev/null && pwd )

: "${DATABASE_URL:?DATABASE_URL is not set}"

# Drop scheme and the "user@" prefix, but KEEP the query string
rest="${DATABASE_URL#*://}"
hostpart="${rest#*@}"   # unchanged if there is no '@'

# pgjdbc can't read Cockroach's PEM key; convert to PKCS#8 DER
if [[ "$hostpart" =~ sslkey=([^\&]+) ]]; then
  KEY="${BASH_REMATCH[1]}"
  PK8="$(mktemp -d)/client.key.pk8"
  openssl pkcs8 -topk8 -inform PEM -outform DER \
    -in "$KEY" -out "$PK8" -nocrypt
  hostpart="${hostpart/sslkey=$KEY/sslkey=$PK8}"
fi

JDBC_URL="jdbc:postgresql://${hostpart}&tcpKeepAlive=true&socketTimeout=0&connectTimeout=30"

flyway \
  "-url=${JDBC_URL}" \
  -user=root \
  "-locations=filesystem:${SCRIPTDIR}/../migrations" \
  -executeInTransaction=false \
  migrate