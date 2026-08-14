#!/bin/bash
# Start the full agent governance ecosystem locally.
#
# Starts: Patroclus (8484), Miser (8787), Relay (8090), Hive (8000),
#         Sentiel (8585), Aegis (8686)
#
# Usage:
#   ./start-ecosystem.sh start    # Start everything
#   ./start-ecosystem.sh status   # Check status
#   ./start-ecosystem.sh stop     # Stop everything

set -e

PATROCLUS_DIR=~/patroclus
HIVE_DIR=~/hive
RELAY_DIR=~/relay
MISER_DIR=~/miser
SENTIEL_DIR=~/sentiel
AEGIS_DIR=~/Aegis

wait_for() {
    local name=$1 url=$2 tries=0
    while [ $tries -lt 30 ]; do
        if curl -s --max-time 2 "$url" 2>/dev/null | grep -qi "ok\|alive\|ready\|connected"; then
            echo "  ✓ $name is up"
            return 0
        fi
        sleep 1
        tries=$((tries + 1))
    done
    echo "  ✗ $name failed to start"
    return 1
}

start_service() {
    local name=$1 port=$2 pidfile=$3
    echo "Starting $name (port $port)..."
    echo $! > "$pidfile"
}

start_all() {
    echo ""
    echo "╔══════════════════════════════════════════════════╗"
    echo "║   Agent Governance Ecosystem — Starting          ║"
    echo "║   Hive + Patroclus + Relay + Miser              ║"
    echo "║   + Sentiel + Aegis                              ║"
    echo "╚══════════════════════════════════════════════════╝"
    echo ""

    # Patroclus
    echo "Starting Patroclus (8484)..."
    cd "$PATROCLUS_DIR" && rm -f patroclus.db
    nohup ./target/release/patroclus serve --config config.toml > /tmp/patroclus.log 2>&1 &
    echo $! > /tmp/patroclus.pid
    wait_for "Patroclus" "http://localhost:8484/health"

    # Miser
    echo "Starting Miser (8787)..."
    cd "$MISER_DIR"
    nohup ./target/release/miser-gateway > /tmp/miser.log 2>&1 &
    echo $! > /tmp/miser.pid
    wait_for "Miser" "http://localhost:8787/health/live"

    # Sentiel
    echo "Starting Sentiel (8585)..."
    cd "$SENTIEL_DIR" && rm -f sentiel.db
    nohup ./target/release/sentiel serve > /tmp/sentiel.log 2>&1 &
    echo $! > /tmp/sentiel.pid
    wait_for "Sentiel" "http://localhost:8585/health"

    # Aegis
    echo "Starting Aegis (8686)..."
    cd "$AEGIS_DIR" && rm -f aegis.db
    nohup ./target/release/aegis serve > /tmp/aegis.log 2>&1 &
    echo $! > /tmp/aegis.pid
    wait_for "Aegis" "http://localhost:8686/health"

    # Relay
    echo "Starting Relay (8090)..."
    cd "$RELAY_DIR"
    RELAY_SERVER__PORT=8090 PATROCLUS_ENABLED=true PATROCLUS_URL=http://localhost:8484 \
    RELAY_ALLOW_DEFAULT_SECRET=1 OAUTH__JWT_SECRET_KEY=relay-dev-secret \
    nohup python -m gateway.server http > /tmp/relay.log 2>&1 &
    echo $! > /tmp/relay.pid
    sleep 3
    wait_for "Relay" "http://localhost:8090/patroclus/status"

    # Hive
    echo "Starting Hive (8000)..."
    cd "$HIVE_DIR/backend"
    nohup python -m uvicorn main:app --port 8000 > /tmp/hive.log 2>&1 &
    echo $! > /tmp/hive.pid
    sleep 4
    if curl -s --max-time 2 http://localhost:8000/ > /dev/null; then
        echo "  ✓ Hive is up"
    else
        echo "  ✗ Hive failed to start"
    fi

    echo ""
    echo "All services started!"
    status_all
    echo ""
    echo "Dashboards:"
    echo "  Patroclus:  http://localhost:8484/health"
    echo "  Sentiel:    http://localhost:8585/ (dashboard)"
    echo "  Aegis:      http://localhost:8686/health"
    echo "  Hive:       http://localhost:8000/ (marketplace)"
    echo ""
    echo "Logs: /tmp/{patroclus,miser,sentiel,aegis,relay,hive}.log"
}

stop_all() {
    echo "Stopping all services..."
    for f in /tmp/patroclus.pid /tmp/miser.pid /tmp/sentiel.pid /tmp/aegis.pid /tmp/relay.pid /tmp/hive.pid; do
        [ -f "$f" ] && kill "$(cat $f)" 2>/dev/null && echo "  Stopped $(cat $f)" && rm -f "$f"
    done
}

status_all() {
    echo ""
    printf "  %-12s %-6s %-10s %s\n" "SERVICE" "PORT" "STATUS" "URL"
    printf "  %-12s %-6s %-10s %s\n" "-------" "----" "------" "---"
    for svc in "Patroclus:8484:http://localhost:8484/health" \
               "Miser:8787:http://localhost:8787/health/live" \
               "Sentiel:8585:http://localhost:8585/health" \
               "Aegis:8686:http://localhost:8686/health" \
               "Relay:8090:http://localhost:8090/patroclus/status" \
               "Hive:8000:http://localhost:8000/"; do
        name=$(echo "$svc" | cut -d: -f1)
        port=$(echo "$svc" | cut -d: -f2)
        url=$(echo "$svc" | cut -d: -f3-)
        if curl -s --max-time 2 "$url" 2>/dev/null | grep -qi "ok\|alive\|ready\|connected\|<!DOCTYPE\|<html"; then
            printf "  %-12s %-6s %-10s %s\n" "$name" "$port" "✓ UP" "$url"
        else
            printf "  %-12s %-6s %-10s %s\n" "$name" "$port" "✗ DOWN" "$url"
        fi
    done
    echo ""
}

case "${1:-start}" in
    start) start_all ;;
    stop) stop_all ;;
    status) status_all ;;
    *) echo "Usage: $0 {start|stop|status}"; exit 1 ;;
esac
