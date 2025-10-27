#!/bin/bash
# Start script for Rust WebSocket latency measurement client

set -e

# Check if binary exists
if [ ! -f "target/release/bn-ws-latency-rust" ]; then
    echo "Binary not found. Building..."
    ./build.sh
fi

# Default options
ENABLE_LOGS="-wlogs"
WS_URI="wss://fstream.binance.com:443/ws/stream"

# Parse arguments
while [[ $# -gt 0 ]]; do
    case $1 in
        -wlogs|--enable-logs)
            ENABLE_LOGS="-wlogs"
            shift
            ;;
        wss://*|ws://*)
            WS_URI="$1"
            shift
            ;;
        *)
            echo "Unknown option: $1"
            echo "Usage: $0 [-wlogs] [ws://... or wss://...]"
            exit 1
            ;;
    esac
done

# Check if RocketMQ is available
if ! nc -z localhost 9876 2>/dev/null; then
    echo "WARNING: RocketMQ not detected on localhost:9876"
fi

# Run the client in background
export RUST_LOG=info
./target/release/bn-ws-latency-rust $ENABLE_LOGS $WS_URI > output.log 2>&1 &

echo "Started in background with PID: $!"
echo "Log file: output.log"
echo "To stop: kill $!"
