#!/usr/bin/env bash
# Start the Rust API and the Next.js dev server together (macOS/Linux/Git Bash/WSL).
# Ctrl+C stops both.
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"

if [[ ! -f "$ROOT/.env" ]]; then
  echo "No .env found. Creating one from .env.example - set API_TOKEN before going further." >&2
  cp "$ROOT/.env.example" "$ROOT/.env"
fi

# Export every KEY=value in .env so both processes see the same config.
set -a
# shellcheck disable=SC1091
. "$ROOT/.env"
set +a

if [[ ! -d "$ROOT/apps/web/node_modules" ]]; then
  echo "Installing web dependencies..."
  (cd "$ROOT/apps/web" && if [[ -f package-lock.json ]]; then npm ci; else npm install; fi)
fi

cleanup() {
  trap - INT TERM EXIT
  echo
  echo "Stopping..."
  kill 0 2>/dev/null || true
}
trap cleanup INT TERM EXIT

echo "API -> http://localhost:8080   Web -> http://localhost:3000"
(cd "$ROOT/services/api" && cargo run -p api-server -- serve) &
(cd "$ROOT/apps/web" && npm run dev) &
wait
