#!/usr/bin/env bash
set -euo pipefail

repo="${1:-}"
interval="${2:-2}"

if [[ -z "$repo" ]]; then
  echo "Usage: scripts/run-car.sh <repo-path> [interval-seconds]"
  exit 1
fi

if [[ ! -d "$repo/.git" ]]; then
  echo "Not a git repository: $repo"
  exit 1
fi

echo "Starting Continuous Architecture Runtime for $repo"

echo "Initializing cold start artifacts..."
scripts/cold-start-repos.sh --baseline-scan "$repo"

echo "Starting continuous watch + CAR projections..."
exec cargo run -p treehouse-mock --bin treehouse -- watch "$repo" --interval "$interval" --continuous
