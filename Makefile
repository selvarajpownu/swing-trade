# swing-trade developer commands (macOS/Linux, or Windows with Git Bash/WSL).
# On plain Windows PowerShell use scripts/dev.ps1 and the commands in README.md.

SHELL := /bin/sh
ROOT  := $(abspath $(dir $(lastword $(MAKEFILE_LIST))))
API   := $(ROOT)/services/api
WEB   := $(ROOT)/apps/web

# Export every KEY=value in the root .env into the recipe's shell (if the file exists).
LOAD_ENV := set -a; if [ -f "$(ROOT)/.env" ]; then . "$(ROOT)/.env"; fi; set +a;

.DEFAULT_GOAL := help
.PHONY: help dev-api dev-web login test fmt lint build docker-up docker-down

help: ## Show this help
	@awk 'BEGIN {FS = ":.*##"; printf "Usage: make <target>\n\n"} /^[a-zA-Z_-]+:.*##/ {printf "  \033[36m%-12s\033[0m %s\n", $$1, $$2}' $(MAKEFILE_LIST)

$(WEB)/node_modules: $(WEB)/package.json
	cd "$(WEB)" && if [ -f package-lock.json ]; then npm ci; else npm install; fi
	@touch "$@"

dev-api: ## Run the Rust API on :8080 (reads .env)
	$(LOAD_ENV) cd "$(API)" && cargo run -p api-server -- serve

dev-web: $(WEB)/node_modules ## Run the Next.js dev server on :3000 (reads .env)
	$(LOAD_ENV) cd "$(WEB)" && npm run dev

login: ## Log in to INDmoney (OAuth) and save the token file
	$(LOAD_ENV) cd "$(API)" && cargo run -p api-server -- login indmoney

test: $(WEB)/node_modules ## Run Rust tests and web typecheck + lint
	cd "$(API)" && cargo test --workspace
	cd "$(WEB)" && npm run typecheck && npm run lint

fmt: ## Format Rust code
	cd "$(API)" && cargo fmt --all

lint: $(WEB)/node_modules ## Check formatting, clippy (warnings = errors) and web lint
	cd "$(API)" && cargo fmt --all -- --check
	cd "$(API)" && cargo clippy --workspace --all-targets -- -D warnings
	cd "$(WEB)" && npm run lint

build: $(WEB)/node_modules ## Release build of the API and production build of the web app
	cd "$(API)" && cargo build --release -p api-server
	$(LOAD_ENV) cd "$(WEB)" && npm run build

docker-up: ## Build and start api + web with Docker Compose
	docker compose up --build -d

docker-down: ## Stop and remove the Compose containers
	docker compose down
