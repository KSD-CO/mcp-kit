.PHONY: all check fmt clippy test build clean ci fix

# Default target
all: check

# ─── CI Check (run before push) ────────────────────────────────────────────────

ci: fmt-check clippy test
	@echo "✅ All CI checks passed!"

check: fmt-check clippy
	@echo "✅ Code checks passed!"

# ─── Formatting ────────────────────────────────────────────────────────────────

fmt:
	cargo fmt --all

fmt-check:
	@echo "🔍 Checking formatting..."
	cargo fmt --all -- --check

# ─── Linting ───────────────────────────────────────────────────────────────────

clippy:
	@echo "🔍 Running clippy..."
	cargo clippy --workspace --all-features -- -D warnings

clippy-fix:
	cargo clippy --workspace --all-features --fix --allow-dirty

# ─── Testing ───────────────────────────────────────────────────────────────────

test:
	@echo "🧪 Running tests..."
	cargo test --workspace --all-features

# ─── Build ─────────────────────────────────────────────────────────────────────

build:
	cargo build --workspace --all-features

build-release:
	cargo build --workspace --all-features --release

build-examples:
	cargo build --examples

# ─── Auto-fix all issues ───────────────────────────────────────────────────────

fix: fmt clippy-fix
	@echo "✅ Auto-fix complete!"

# ─── Clean ─────────────────────────────────────────────────────────────────────

clean:
	cargo clean

# ─── Cloudflare Deploy ─────────────────────────────────────────────────────────

cf-build:
	cd deploy/cloudflare && cargo build

cf-deploy:
	cd deploy/cloudflare && wrangler deploy

# ─── Publish ───────────────────────────────────────────────────────────────────

publish-dry:
	cargo publish -p mcp-kit-macros --dry-run
	cargo publish -p mcp-kit --dry-run

publish:
	cargo publish -p mcp-kit-macros
	cargo publish -p mcp-kit

# ─── Help ──────────────────────────────────────────────────────────────────────

help:
	@echo "Available targets:"
	@echo "  make ci          - Run all CI checks (fmt, clippy, test)"
	@echo "  make check       - Run quick checks (fmt, clippy)"
	@echo "  make fix         - Auto-fix formatting and lint issues"
	@echo "  make fmt         - Format code"
	@echo "  make clippy      - Run clippy linter"
	@echo "  make test        - Run tests"
	@echo "  make build       - Build workspace"
	@echo "  make cf-deploy   - Deploy to Cloudflare Workers"
	@echo "  make clean       - Clean build artifacts"
