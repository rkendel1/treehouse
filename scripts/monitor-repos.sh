#!/usr/bin/env bash
set -euo pipefail

# Runs one Treehouse watch loop per repo and prefixes output by repo name.
interval=2
iterations=""
logs_dir=".treehouse/monitors"
desktop_all=false
desktop_max=""

usage() {
  cat <<'EOF'
Usage:
  scripts/monitor-repos.sh [--interval secs] [--iterations n] [--logs dir] [--desktop-all] [--desktop-max n] <repo-path> [repo-path...]

Examples:
  scripts/monitor-repos.sh --interval 2 --iterations 1 ~/Desktop/treehouse
  scripts/monitor-repos.sh --interval 3 ~/Desktop/repo-a ~/Desktop/repo-b
  scripts/monitor-repos.sh --desktop-all --desktop-max 5
EOF
}

repos=()
while (($# > 0)); do
  case "$1" in
    --interval)
      shift
      interval="${1:-}"
      ;;
    --iterations)
      shift
      iterations="${1:-}"
      ;;
    --logs)
      shift
      logs_dir="${1:-}"
      ;;
    --desktop-all)
      desktop_all=true
      ;;
    --desktop-max)
      shift
      desktop_max="${1:-}"
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
    repo_path="${gitdir%/.git}"
    repos+=("$repo_path")
  done < <(find "$HOME/Desktop" -maxdepth 3 -type d -name .git | sort -u)
fi

if [[ -n "$desktop_max" ]]; then
  if ! [[ "$desktop_max" =~ ^[0-9]+$ ]]; then
    echo "--desktop-max must be a positive integer"
    exit 1
  fi
fi

if ((${#repos[@]} == 0)); then
  usage
  exit 1
fi

if [[ -n "$desktop_max" ]]; then
  limited_repos=()
  count=0
  for repo in "${repos[@]}"; do
    limited_repos+=("$repo")
    count=$((count + 1))
    if ((count >= desktop_max)); then
      break
    fi
  done
  repos=("${limited_repos[@]}")
fi

pids=()
cleanup() {
  echo
  echo "Stopping monitors..."
  for pid in "${pids[@]}"; do
    kill "$pid" 2>/dev/null || true
  done
}
trap cleanup INT TERM

for repo in "${repos[@]}"; do
  if [[ ! -d "$repo/.git" ]]; then
    echo "Skipping $repo (not a git repo)"
    continue
  fi

  name="$(basename "$repo")"
  if [[ "$logs_dir" = /* ]]; then
    repo_logs_dir="$logs_dir"
  else
    repo_logs_dir="$repo/$logs_dir"
  fi
  mkdir -p "$repo_logs_dir"
  log_file="$repo_logs_dir/${name}.log"

  cmd=(cargo run -p treehouse-mock --bin treehouse -- watch "$repo" --interval "$interval")
  if [[ -n "$iterations" ]]; then
    cmd+=(--iterations "$iterations")
  else
    cmd+=(--continuous)
  fi

  (
    "${cmd[@]}" 2>&1 | sed -u "s/^/[$name] /" | tee -a "$log_file"
  ) &
  pid=$!
  pids+=("$pid")

  echo "Started $name (pid $pid), log: $log_file"
done

if ((${#pids[@]} == 0)); then
  echo "No monitors were started."
  exit 1
fi

echo "Monitoring ${#pids[@]} repo(s). Press Ctrl+C to stop all."
wait
