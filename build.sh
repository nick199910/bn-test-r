#!/bin/bash
# Build script for Rust WebSocket latency measurement client
# Uses sonic-rs for high-performance SIMD JSON parsing

set -e

# Check if Rust is installed
if ! command -v cargo &> /dev/null; then
    echo "Error: Rust/Cargo not found. Please install Rust:"
    echo "  curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh"
    exit 1
fi

# Build in release mode for maximum performance
cargo build --release

echo "Build complete: target/release/bn-ws-latency-rust"
