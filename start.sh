#!/bin/bash
# Start script for Rust WebSocket latency measurement client

set -e

echo "Starting Rust WebSocket Latency Measurement Client"
echo "===================================================="

# Check if binary exists
if [ ! -f "target/release/bn-ws-latency-rust" ]; then
    echo "Binary not found. Building..."
    ./build.sh
fi

# Default options
ENABLE_LOGS=""
WS_URI="wss://fstream.binance.com:443/ws/stream"

# Parse arguments
while [[ $# -gt 0 ]]; do
    case $1 in
        -wlogs|--enable-logs)
            ENABLE_LOGS="-wlogs"
            echo "Async logging: ENABLED"
            shift
            ;;
        wss://*|ws://*)
            WS_URI="$1"
            echo "WebSocket URI: $WS_URI"
            shift
            ;;
        *)
            echo "Unknown option: $1"
            echo "Usage: $0 [-wlogs] [ws://... or wss://...]"
            exit 1
            ;;
    esac
done

echo ""
echo "Configuration:"
echo "  - WebSocket URI: $WS_URI"
echo "  - Async Logging: ${ENABLE_LOGS:-DISABLED}"
echo ""

# Check if RocketMQ is available
if ! nc -z localhost 9876 2>/dev/null; then
    echo "WARNING: RocketMQ not detected on localhost:9876"
    echo "         MSK latency measurement will be skipped"
    echo "         To enable: Install and start RocketMQ"
    echo ""
fi

# Run the client
echo "Starting client (Press Ctrl+C to stop)..."
echo ""

export RUST_LOG=info
./target/release/bn-ws-latency-rust $ENABLE_LOGS $WS_URI
