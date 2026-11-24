set windows-shell := ["powershell.exe", "-NoProfile", "-Command"]

# MagpieCraft automation (Pure-Rust ArtCraft pattern)

default:
    @just --list

# Strict lint gate across all workspace members
check:
    $env:Path = "$env:USERPROFILE\.cargo\bin;$env:Path"; cargo clippy --workspace --all-targets --all-features -- -D warnings

# Unit and integration tests
test:
    $env:Path = "$env:USERPROFILE\.cargo\bin;$env:Path"; cargo test --workspace

# Formatting gate
fmt-check:
    $env:Path = "$env:USERPROFILE\.cargo\bin;$env:Path"; cargo fmt --all --check

# All gates in one command
ci: fmt-check check test

# MCP stdio server (a client normally starts this; useful for a manual probe)
run-mcp:
    $env:Path = "$env:USERPROFILE\.cargo\bin;$env:Path"; cargo run -q -p magpiecraft-mcp -- mcp

# CLI smoke: status, libraries, stats
run-cli:
    $env:Path = "$env:USERPROFILE\.cargo\bin;$env:Path"; cargo run -q -p magpiecraft-mcp -- status --json

# Desktop app (egui)
run-app:
    $env:Path = "$env:USERPROFILE\.cargo\bin;$env:Path"; cargo run -q -p magpiecraft-app

# Headless app smoke (no window): opens the catalog, browses the first library, exits
smoke:
    $env:Path = "$env:USERPROFILE\.cargo\bin;$env:Path"; cargo run -q -p magpiecraft-app -- --smoke

# Synthetic 100,000-file scan benchmark (writes only under data/bench)
bench:
    $env:Path = "$env:USERPROFILE\.cargo\bin;$env:Path"; cargo run -q --release -p magpiecraft-mcp -- bench-scan --files 100000 --json

# Production build
build-release:
    $env:Path = "$env:USERPROFILE\.cargo\bin;$env:Path"; cargo build --release --workspace
