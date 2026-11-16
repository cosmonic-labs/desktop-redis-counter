#!/usr/bin/env bash
# Start a throwaway Redis for this sample, on 127.0.0.1:6379 only.
# Usage: scripts/start-redis.sh [port]
set -euo pipefail
PORT="${1:-6379}"
NAME="cosmonic-demo-redis"
IMAGE="redis:7.4-alpine"

command -v docker >/dev/null || { echo "docker is required (Docker Desktop, OrbStack, colima...)" >&2; exit 1; }

if docker ps --format '{{.Names}}' | grep -qx "$NAME"; then
  echo "$NAME is already running at $(docker port "$NAME" 6379/tcp | head -1)"
  exit 0
fi
if (exec 3<>"/dev/tcp/127.0.0.1/$PORT") 2>/dev/null; then
  echo "Something is already listening on 127.0.0.1:$PORT." >&2
  echo "If it is a Redis you want to use, skip this script; otherwise pass another port." >&2
  exit 1
fi

docker rm -f "$NAME" >/dev/null 2>&1 || true
# Loopback only: nothing outside this machine can reach it. No persistence.
docker run -d --name "$NAME" -p "127.0.0.1:$PORT:6379" "$IMAGE" \
  redis-server --save '' --appendonly no >/dev/null

for _ in $(seq 1 30); do
  if docker exec "$NAME" redis-cli ping 2>/dev/null | grep -q PONG; then
    echo "Redis is up: redis://127.0.0.1:$PORT"
    echo "Use that URL for the Redis binding named 'cache' in Cosmonic Desktop."
    exit 0
  fi
  sleep 1
done
echo "Redis did not answer; see: docker logs $NAME" >&2
exit 1
