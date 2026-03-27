#!/bin/bash
# Dependency checker script for RustShare
# Usage: ./scripts/check-dependencies.sh [--update]

set -e

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(dirname "$SCRIPT_DIR")"
UPDATE=false

# Parse arguments
if [ "$1" == "--update" ]; then
    UPDATE=true
fi

echo "=========================================="
echo "🔍 Checking RustShare Dependencies"
echo "=========================================="

# Check if cargo-outdated is installed
check_cargo_outdated() {
    if ! command -v cargo-outdated &> /dev/null; then
        echo "❌ cargo-outdated not found. Installing..."
        cargo install cargo-outdated
    fi
}

# Check if cargo-audit is installed
check_cargo_audit() {
    if ! command -v cargo-audit &> /dev/null; then
        echo "❌ cargo-audit not found. Installing..."
        cargo install cargo-audit
    fi
}

echo ""
echo "📦 Backend (Rust) Dependencies"
echo "--------------------------------"
cd "$PROJECT_ROOT/backend"

check_cargo_outdated

echo ""
echo "➡️  Checking for outdated dependencies..."
cargo outdated -R

if [ "$UPDATE" = true ]; then
    echo ""
    echo "➡️  Updating dependencies..."
    cargo update
    
    echo ""
    echo "➡️  Verifying build..."
    cargo check --all-features
    
    echo ""
    echo "✅ Dependencies updated and build verified!"
fi

echo ""
echo "🔒 Running security audit..."
check_cargo_audit
cargo audit || true

echo ""
echo "📦 Frontend (Node.js) Dependencies"
echo "-----------------------------------"
cd "$PROJECT_ROOT/frontend"

if command -v npm &> /dev/null; then
    echo ""
    echo "➡️  Checking for outdated dependencies..."
    npm outdated || true
    
    echo ""
    echo "🔒 Running security audit..."
    npm audit --audit-level=high || true
else
    echo "⚠️  npm not found. Skipping frontend check."
fi

echo ""
echo "=========================================="
echo "✅ Dependency check complete!"
echo "=========================================="

if [ "$UPDATE" = false ]; then
    echo ""
    echo "💡 To update dependencies, run:"
    echo "   ./scripts/check-dependencies.sh --update"
fi
