#!/bin/bash
# Helper script to test SIGINT handling
# This script runs cctr and sends it SIGINT, then reports results

set -e

FIXTURE_DIR="$1"
rm -f /tmp/cctr_signal_sync_*

# Run cctr in background
cctr "$FIXTURE_DIR/signal_sync" --no-color &
CCTR_PID=$!

# Poll until test1 signals it has started (max 5 seconds)
for i in $(seq 1 50); do
  if [ -f /tmp/cctr_signal_sync_test1_started ]; then
    break
  fi
  sleep 0.1
done

# Send SIGINT
kill -INT $CCTR_PID 2>/dev/null || true

# Wait for cctr to finish
wait $CCTR_PID 2>/dev/null || true

# Report results
echo "teardown_exists=$(test -f /tmp/cctr_signal_sync_teardown && echo yes || echo no)"
echo "test1_started=$(test -f /tmp/cctr_signal_sync_test1_started && echo yes || echo no)"
echo "test2_ran=$(test -f /tmp/cctr_signal_sync_test2 && echo yes || echo no)"
