default:
    @just --list

# Build (rquickjs, debug)
build:
    cargo build

# Build with boa engine
build-boa:
    cargo build --no-default-features --features boa

# Run tests (rquickjs)
test:
    cargo test

# Run tests with boa engine
test-boa:
    cargo test --no-default-features --features boa

# Lint (both engines)
clippy:
    cargo clippy -- -D warnings
    cargo clippy --no-default-features --features boa -- -D warnings

# Format
fmt:
    cargo fmt

# Check formatting without modifying
fmt-check:
    cargo fmt --check

# Run TodoMVC compatibility sweep
todomvc:
    bash scripts/todomvc_compat.sh

# Download and smoke-test the latest release binary
smoke:
    bash scripts/smoke_test_release.sh

# Smoke-test a local binary (usage: just smoke-local ./target/release/rakers)
smoke-local bin:
    bash scripts/smoke_test_release.sh {{bin}}

# Tag and push a release (usage: just release v0.2.0)
release tag:
    git tag {{tag}}
    git push origin {{tag}}

# Dry-run publish to crates.io
publish-dry:
    cargo publish --dry-run

# Publish to crates.io
publish:
    cargo publish
