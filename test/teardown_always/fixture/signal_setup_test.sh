#!/bin/bash
# Helper script to test SIGINT handling during _setup.txt
# Runs cctr and sends SIGINT while setup is running, then reports results

FIXTURE_DIR="$1"
rm -f /tmp/cctr_signal_setup_*

# Run cctr in background
cctr "$FIXTURE_DIR/signal_setup" --no-color &
CCTR_PID=$!

# Poll until setup signals it has started (max 10 seconds)
started=false
for i in $(seq 1 100); do
  if [ -f /tmp/cctr_signal_setup_started ]; then
    started=true
    sleep 0.1
    break
  fi
  sleep 0.1
done

if [ "$started" != "true" ]; then
  echo "ERROR: setup never started"
  kill $CCTR_PID 2>/dev/null || true
  exit 1
fi

# Send SIGINT while setup is still running (sleep 10)
kill -INT $CCTR_PID 2>/dev/null || true

# Wait for cctr to finish (with timeout)
wait $CCTR_PID 2>/dev/null || true

# Report results
echo "setup_started=$(test -f /tmp/cctr_signal_setup_started && echo yes || echo no)"
echo "setup_step2_ran=$(test -f /tmp/cctr_signal_setup_step2 && echo yes || echo no)"
echo "teardown_ran=$(test -f /tmp/cctr_signal_setup_teardown && echo yes || echo no)"
echo "main_ran=$(test -f /tmp/cctr_signal_setup_main && echo yes || echo no)"
