#!/bin/bash
# Start the full agent governance ecosystem locally.
#
# Starts: Patroclus (8484), Miser (8787), Relay (8090), Hive (8000)
#
# Usage:
#   ./start-ecosystem.sh          # Start everything
#   ./start-ecosystem.sh status   # Check status of all services
#   ./start-ecosystem.sh stop     # Stop everything

set -e

PATROCLUS_DIR=~/patroclus
HIVE_DIR=~/hive
RELAY_DIR=~/relay
MISER_DIR=~/miser

PATROCLUS_PORT=8484
MISER_PORT=8787
RELAY_PORT=8090
HIVE_PORT=8000

wait_for() {
    local name=$1
    local url=$2
    local tries=0
    while [ $tries -lt 30 ]; do
        if curl -s --max-time 2 "$url" | grep -q "ok\|alive\|ready"; then
            echo "  ✓ $name is up"
            return 0
        fi
        sleep 1
        tries=$((tries + 1))
    done
    echo "  ✗ $name failed to start (timeout)"
    return 1
}

start_patroclus() {
    echo "Starting Patroclus (port $PATROCLUS_PORT)..."
    cd "$PATROCLUS_DIR"
    rm -f patroclus.db
    nohup ./target/release/patroclus serve --config config.toml > /tmp/patroclus.log 2>&1 &
    echo $! > /tmp/patroclus.pid
    wait_for "Patroclus" "http://localhost:$PATROCLUS_PORT/health"
}

start_miser() {
    echo "Starting Miser (port $MISER_PORT)..."
    cd "$MISER_DIR"
    nohup ./target/release/miser > /tmp/miser.log 2>&1 &
    echo $! > /tmp/miser.pid
    wait_for "Miser" "http://localhost:$MISER_PORT/health/live"
}

start_relay() {
    echo "Starting Relay (port $RELAY_PORT)..."
    cd "$RELAY_DIR"
    RELAY_SERVER__PORT=$RELAY_PORT \
    PATROCLUS_ENABLED=true \
    PATROCLUS_URL=http://localhost:$PATROCLUS_PORT \
    RELAY_ALLOW_DEFAULT_SECRET=1 \
    OAUTH__JWT_SECRET_KEY=relay-dev-secret-change-in-production \
    nohup python -m gateway.server http > /tmp/relay.log 2>&1 &
    echo $! > /tmp/relay.pid
    sleep 3
    wait_for "Relay" "http://localhost:$RELAY_PORT/patroclus/status"
}

start_hive() {
    echo "Starting Hive (port $HIVE_PORT)..."
    cd "$HIVE_DIR/backend"
    nohup python -m uvicorn main:app --port $HIVE_PORT > /tmp/hive.log 2>&1 &
    echo $! > /tmp/hive.pid
    wait_for "Hive" "http://localhost:$HIVE_PORT/health"
}

stop_all() {
    echo "Stopping all services..."
    for pidfile in /tmp/patroclus.pid /tmp/miser.pid /tmp/relay.pid /tmp/hive.pid; do
        if [ -f "$pidfile" ]; then
            pid=$(cat "$pidfile")
            kill "$pid" 2>/dev/null && echo "  Stopped PID $pid"
            rm -f "$pidfile"
        fi
    done
}

status_all() {
    echo "Service Status:"
    echo "─────────────────────────────────────────────"

    printf "  %-12s %-6s %-10s %s\n" "SERVICE" "PORT" "STATUS" "URL"
    printf "  %-12s %-6s %-10s %s\n" "-------" "----" "------" "---"

    for svc in "Patroclus:$PATROCLUS_PORT:http://localhost:$PATROCLUS_PORT/health" \
               "Miser:$MISER_PORT:http://localhost:$MISER_PORT/health/live" \
               "Relay:$RELAY_PORT:http://localhost:$RELAY_PORT/patroclus/status" \
               "Hive:$HIVE_PORT:http://localhost:$HIVE_PORT/health"; do
        name=$(echo "$svc" | cut -d: -f1)
        port=$(echo "$svc" | cut -d: -f2)
        url=$(echo "$svc" | cut -d: -f3-)
        if curl -s --max-time 2 "$url" | grep -q "ok\|alive\|ready\|connected"; then
            printf "  %-12s %-6s %-10s %s\n" "$name" "$port" "✓ UP" "$url"
        else
            printf "  %-12s %-6s %-10s %s\n" "$name" "$port" "✗ DOWN" "$url"
        fi
    done
    echo ""
}

case "${1:-start}" in
    start)
        echo ""
        echo "╔══════════════════════════════════════════════════╗"
        echo "║   Agent Governance Ecosystem — Starting          ║"
        echo "║   Hive + Patroclus + Relay + Miser              ║"
        echo "╚══════════════════════════════════════════════════╝"
        echo ""
        start_patroclus
        start_miser
        start_relay
        start_hive
        echo ""
        echo "All services started!"
        echo ""
        status_all
        echo "Logs:"
        echo "  Patroclus: /tmp/patroclus.log"
        echo "  Miser:     /tmp/miser.log"
        echo "  Relay:     /tmp/relay.log"
        echo "  Hive:      /tmp/hive.log"
        echo ""
        ;;
    stop)
        stop_all
        ;;
    status)
        status_all
        ;;
    *)
        echo "Usage: $0 {start|stop|status}"
        exit 1
        ;;
esac
