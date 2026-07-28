#!/usr/bin/env bash
set -euo pipefail

# Launches projections and gateway from repo-state artifacts.
mode="all"
output_dir=""
cold_start=true
baseline_scan=true
repo=""

usage() {
  cat <<'EOF'
Usage:
  scripts/launch-projection.sh <repo-path> [--mode gateway|all|postgres|convex] [--output dir] [--skip-cold-start] [--skip-baseline-scan]

Examples:
  scripts/launch-projection.sh ~/Desktop/repo-a --mode all
  scripts/launch-projection.sh ~/Desktop/repo-a --mode gateway
  scripts/launch-projection.sh ~/Desktop/repo-a --mode postgres --output ~/Desktop/repo-a/.treehouse/projection
EOF
}

while (($# > 0)); do
  case "$1" in
    --mode)
      shift
      mode="${1:-}"
      ;;
    --output)
      shift
      output_dir="${1:-}"
      ;;
    --skip-cold-start)
      cold_start=false
      ;;
    --skip-baseline-scan)
      baseline_scan=false
      ;;
    -h|--help)
      usage
      exit 0
      ;;
    *)
      if [[ -z "$repo" ]]; then
        repo="$1"
      else
        echo "Unexpected argument: $1"
        usage
        exit 1
      fi
      ;;
  esac
  shift || true
done

if [[ -z "$repo" ]]; then
  usage
  exit 1
fi

if [[ ! -d "$repo/.git" ]]; then
  echo "Not a git repository: $repo"
  exit 1
fi

case "$mode" in
  gateway|all|postgres|convex)
    ;;
  *)
    echo "Invalid --mode '$mode'. Use gateway|all|postgres|convex"
    exit 1
    ;;
esac

if [[ "$cold_start" == "true" ]]; then
  cold_start_cmd=(scripts/cold-start-repos.sh)
  if [[ "$baseline_scan" == "true" ]]; then
    cold_start_cmd+=(--baseline-scan)
  fi
  cold_start_cmd+=("$repo")
  echo "Running cold start for $repo..."
  "${cold_start_cmd[@]}"
fi

model_path="$repo/.treehouse/scan/bootstrap/baseline/application-model.json"
if [[ ! -f "$model_path" ]]; then
  if [[ "$baseline_scan" == "true" ]]; then
    echo "Model artifact missing, generating baseline scan..."
    cargo run -p treehouse-mock --bin treehouse -- \
      scan "$repo" \
      --baseline-only \
      --output "$repo/.treehouse/scan/bootstrap" \
      --format json
  else
    echo "Missing model artifact: $model_path"
    echo "Run with baseline scan enabled or generate a model first."
    exit 1
  fi
fi

project_args=(cargo run -p treehouse-mock --bin treehouse -- project "$model_path" --target)

if [[ -n "$output_dir" ]]; then
  case "$mode" in
    all)
      project_args+=(all --output "$output_dir")
      ;;
    postgres)
      project_args+=(postgres --output "$output_dir/postgres")
      ;;
    convex)
      project_args+=(convex --output "$output_dir/convex")
      ;;
    gateway)
      project_args+=(gateway)
      ;;
  esac
else
  project_args+=("$mode")
fi

echo "Launching projection mode '$mode' for $repo..."
"${project_args[@]}"
