#!/usr/bin/env bash
set -euo pipefail

if [ "$#" -lt 2 ]; then
  echo "Usage: $0 <worker_url> <github_token>"
  exit 2
fi

WORKER_URL="$1"
TOKEN="$2"
USERNAME="penqguin"

echo "Running smoke test against: $WORKER_URL"

FIRST_HDRS=$(mktemp)
SECOND_HDRS=$(mktemp)

echo "-> First request (expect MISS)"
curl -sS -D "$FIRST_HDRS" -H "Authorization: Bearer $TOKEN" "$WORKER_URL/commits/latest?username=$USERNAME" -o /tmp/_smoke_first.json || true
grep -i "Cache-Control" "$FIRST_HDRS" || true
grep -i "X-Cache" "$FIRST_HDRS" || true
grep -i "CF-Cache-Status" "$FIRST_HDRS" || true

sleep 2

echo "-> Second request (expect HIT)"
curl -sS -D "$SECOND_HDRS" -H "Authorization: Bearer $TOKEN" "$WORKER_URL/commits/latest?username=$USERNAME" -o /tmp/_smoke_second.json || true

if grep -qiE "CF-Cache-Status:\s*HIT" "$SECOND_HDRS" || grep -qiE "X-Cache:\s*HIT" "$SECOND_HDRS"; then
  echo "Smoke test passed: cache HIT observed on second request"
  exit 0
else
  echo "Smoke test failed: cache HIT not observed on second request"
  echo "--- First response headers ---"
  cat "$FIRST_HDRS"
  echo "--- Second response headers ---"
  cat "$SECOND_HDRS"
  exit 1
fi
