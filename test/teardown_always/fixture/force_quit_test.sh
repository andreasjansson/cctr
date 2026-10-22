#!/bin/bash
# A genuinely separate second signal, sent well after the first, must still
# force a quit. Escalation has to keep working now that near-simultaneous
# duplicates are collapsed into one request.
#
# Targets a suite whose teardown is slow, so cctr is still alive and tearing
# down when the second signal arrives.

FIXTURE_DIR="$1"
rm -f /tmp/cctr_slowtd_started /tmp/cctr_slowtd_teardown_done

cctr "$FIXTURE_DIR/signal_slow_td" --no-color > /tmp/cctr_slowtd_out 2>&1 &
CCTR_PID=$!

for _ in $(seq 1 100); do
  [ -f /tmp/cctr_slowtd_started ] && break
  sleep 0.1
done

if [ ! -f /tmp/cctr_slowtd_started ]; then
  echo "ERROR: test never started"
  kill -9 $CCTR_PID 2>/dev/null || true
  exit 1
fi

sleep 0.2
kill -TERM $CCTR_PID 2>/dev/null || true
# Comfortably beyond the duplicate-collapsing window, and while the slow
# teardown is still running
sleep 1.5
kill -TERM $CCTR_PID 2>/dev/null || true

wait $CCTR_PID 2>/dev/null
echo "exit_code=$?"
echo "force_quit=$(grep -q 'Force quit' /tmp/cctr_slowtd_out && echo yes || echo no)"
echo "teardown_completed=$(test -f /tmp/cctr_slowtd_teardown_done && echo yes || echo no)"