#!/bin/bash
# Check if the Bunyan server is running and reachable

PORT_FILE="$HOME/.bunyan/server.port"
PORT=${BUNYAN_PORT:-3333}

if [ -f "$PORT_FILE" ]; then
    PORT=$(cat "$PORT_FILE")
fi

if curl -sf "http://127.0.0.1:$PORT/health" > /dev/null 2>&1; then
    echo "Bunyan server is running on port $PORT"
    exit 0
else
    echo "Bunyan server is not reachable on port $PORT"
    echo "Start it with: bunyan serve --port $PORT"
    exit 1
fi
