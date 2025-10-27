#!/bin/bash
# Build script for Rust WebSocket latency measurement client
# Uses sonic-rs for high-performance SIMD JSON parsing

set -e

echo "Building Rust WebSocket client with sonic-rs (SIMD JSON parser)..."
echo "====================================================================="

# Check if Rust is installed
if ! command -v cargo &> /dev/null; then
    echo "Error: Rust/Cargo not found. Please install Rust:"
    echo "  curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh"
    exit 1
fi

# Display sonic-rs optimization info
echo ""
echo "SIMD Optimizations:"
echo "  - Using -C target-cpu=native (configured in .cargo/config.toml)"
echo "  - sonic-rs will use SIMD instructions for JSON parsing"
echo "  - Build on the target machine for best performance"
echo ""

# Build in release mode for maximum performance
echo "Building in release mode..."
cargo build --release

echo ""
echo "Build complete!"
echo ""
echo "Executable location: target/release/bn-ws-latency-rust"
echo ""
echo "Usage examples:"
echo "  ./target/release/bn-ws-latency-rust                    # Basic usage"
echo "  ./target/release/bn-ws-latency-rust -wlogs             # With logging"
echo "  cargo run --release -- -wlogs                          # Using cargo"
echo ""
