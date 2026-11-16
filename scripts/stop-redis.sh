#!/usr/bin/env bash
# Stop and remove the sample Redis (its data goes with it).
set -euo pipefail
docker rm -f cosmonic-demo-redis >/dev/null 2>&1 && echo "Stopped cosmonic-demo-redis" || echo "cosmonic-demo-redis was not running"
