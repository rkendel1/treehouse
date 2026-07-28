#!/usr/bin/env bash
set -euo pipefail

# Bootstraps initial Treehouse artifacts for watched repositories.
bootstrap_interval=1
bootstrap_iterations=1
desktop_all=false
desktop_max=""
run_baseline_scan=false
start_monitor=false
monitor_interval=2

usage() {
  cat <<'EOF'
Usage:
  scripts/cold-start-repos.sh [options] <repo-path> [repo-path...]

Options:
  --desktop-all             Discover git repos on ~/Desktop (max depth 3)
  --desktop-max <n>         Limit number of repos after discovery/list parsing
  --bootstrap-interval <s>  Interval used for bootstrap watch pass (default: 1)
  --bootstrap-iterations <n>Number of initial watch iterations (default: 1)
  --baseline-scan           Run baseline-only scan into .treehouse/scan/bootstrap
  --start-monitor           Start continuous monitoring after bootstrap
  --monitor-interval <s>    Interval for continuous monitor handoff (default: 2)
  -h, --help                Show help

Examples:
  scripts/cold-start-repos.sh ~/Desktop/repo-a ~/Desktop/repo-b
  scripts/cold-start-repos.sh --desktop-all --desktop-max 5 --start-monitor
  scripts/cold-start-repos.sh --baseline-scan ~/Desktop/treehouse
EOF
}

repos=()
while (($# > 0)); do
  case "$1" in
    --desktop-all)
      desktop_all=true
      ;;
    --desktop-max)
      shift
      desktop_max="${1:-}"
      ;;
    --bootstrap-interval)
      shift
      bootstrap_interval="${1:-}"
      ;;
    --bootstrap-iterations)
      shift
      bootstrap_iterations="${1:-}"
      ;;
    --baseline-scan)
      run_baseline_scan=true
      ;;
    --start-monitor)
      start_monitor=true
      ;;
    --monitor-interval)
      shift
      monitor_interval="${1:-}"
      ;;
    -h|--help)
      usage
      exit 0
      ;;
    *)
      repos+=("$1")
      ;;
  esac
  shift || true
done

if [[ "$desktop_all" == "true" ]]; then
  while IFS= read -r gitdir; do
    repos+=("${gitdir%/.git}")
  done < <(find "$HOME/Desktop" -maxdepth 3 -type d -name .git | sort -u)
fi

if ! [[ "$bootstrap_iterations" =~ ^[0-9]+$ ]] || [[ "$bootstrap_iterations" == "0" ]]; then
  echo "--bootstrap-iterations must be a positive integer"
  exit 1
fi
if ! [[ "$bootstrap_interval" =~ ^[0-9]+$ ]]; then
  echo "--bootstrap-interval must be a non-negative integer"
  exit 1
fi
if ! [[ "$monitor_interval" =~ ^[0-9]+$ ]]; then
  echo "--monitor-interval must be a non-negative integer"
  exit 1
fi
if [[ -n "$desktop_max" ]] && ! [[ "$desktop_max" =~ ^[0-9]+$ ]]; then
  echo "--desktop-max must be a positive integer"
  exit 1
fi

if ((${#repos[@]} == 0)); then
  usage
  exit 1
fi

# Deduplicate while preserving order.
unique_repos=()
for repo in "${repos[@]}"; do
  seen=false
  for existing in "${unique_repos[@]:-}"; do
    if [[ "$existing" == "$repo" ]]; then
      seen=true
      break
    fi
  done
  if [[ "$seen" == "false" ]]; then
    unique_repos+=("$repo")
  fi
done
repos=("${unique_repos[@]}")

if [[ -n "$desktop_max" ]]; then
  limited=()
  count=0
  for repo in "${repos[@]}"; do
    limited+=("$repo")
    count=$((count + 1))
    if ((count >= desktop_max)); then
      break
    fi
  done
  repos=("${limited[@]}")
fi

echo "Cold start for ${#repos[@]} repo(s)..."
bootstrapped=()
for repo in "${repos[@]}"; do
  if [[ ! -d "$repo/.git" ]]; then
    echo "Skipping $repo (not a git repo)"
    continue
  fi

  name="$(basename "$repo")"
  treehouse_dir="$repo/.treehouse"
  monitor_dir="$treehouse_dir/monitors"
  bootstrap_dir="$treehouse_dir/bootstrap"
  mkdir -p "$monitor_dir" "$bootstrap_dir"

  bootstrap_log="$monitor_dir/${name}-cold-start.log"
  echo "[$name] Bootstrapping watch artifacts..."
  cargo run -p treehouse-mock --bin treehouse -- \
    watch "$repo" \
    --interval "$bootstrap_interval" \
    --iterations "$bootstrap_iterations" \
    2>&1 | tee "$bootstrap_log"

  if [[ "$run_baseline_scan" == "true" ]]; then
    echo "[$name] Running baseline scan..."
    cargo run -p treehouse-mock --bin treehouse -- \
      scan "$repo" \
      --baseline-only \
      --output "$bootstrap_dir/scan" \
      --format json \
      >> "$bootstrap_log" 2>&1
  fi

  bootstrapped+=("$repo")
  echo "[$name] Cold start complete"
  echo "[$name] Artifacts: $treehouse_dir"
  echo "[$name] Log: $bootstrap_log"
done

if ((${#bootstrapped[@]} == 0)); then
  echo "No repos were bootstrapped."
  exit 1
fi

echo "Cold start complete for ${#bootstrapped[@]} repo(s)."

if [[ "$start_monitor" == "true" ]]; then
  echo "Starting continuous monitor handoff..."
  exec scripts/monitor-repos.sh --interval "$monitor_interval" "${bootstrapped[@]}"
fi
