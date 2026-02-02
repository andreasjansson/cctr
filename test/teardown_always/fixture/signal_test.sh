#!/bin/bash
# Helper script to test SIGINT handling
# This script runs cctr and sends it SIGINT, then reports results

FIXTURE_DIR="$1"
rm -f /tmp/cctr_signal_sync_*

# Run cctr in background
cctr "$FIXTURE_DIR/signal_sync" --no-color &
CCTR_PID=$!

# Poll until test1 signals it has started (max 10 seconds)
started=false
for i in $(seq 1 100); do
  if [ -f /tmp/cctr_signal_sync_test1_started ]; then
    started=true
    # Small extra delay to ensure we're mid-test
    sleep 0.1
    break
  fi
  sleep 0.1
done

if [ "$started" != "true" ]; then
  echo "ERROR: test1 never started"
  kill $CCTR_PID 2>/dev/null || true
  exit 1
fi

# Send SIGINT
kill -INT $CCTR_PID 2>/dev/null || true

# Wait for cctr to finish (with timeout)
wait $CCTR_PID 2>/dev/null || true

# Report results
echo "teardown_exists=$(test -f /tmp/cctr_signal_sync_teardown && echo yes || echo no)"
echo "test1_started=$(test -f /tmp/cctr_signal_sync_test1_started && echo yes || echo no)"
echo "test2_ran=$(test -f /tmp/cctr_signal_sync_test2 && echo yes || echo no)"
