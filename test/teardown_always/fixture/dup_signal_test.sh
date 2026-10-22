#!/bin/bash
# Send the SAME shutdown signal twice in quick succession, the way a single
# event does when it reaches us both through the process group and directly.
# Two deliveries microseconds apart are one request, so teardown must run.
#
# $2 selects the signal (TERM or INT).

FIXTURE_DIR="$1"
SIG="${2:-TERM}"
rm -f /tmp/cctr_sigdup_setup /tmp/cctr_sigdup_teardown /tmp/cctr_sigdup_started

cctr "$FIXTURE_DIR/signal_dup" --no-color > /tmp/cctr_sigdup_out 2>&1 &
CCTR_PID=$!

# Wait until the test is actually running
for _ in $(seq 1 100); do
  [ -f /tmp/cctr_sigdup_started ] && break
  sleep 0.1
done

if [ ! -f /tmp/cctr_sigdup_started ]; then
  echo "ERROR: test never started"
  kill -9 $CCTR_PID 2>/dev/null || true
  exit 1
fi

sleep 0.2
kill -"$SIG" $CCTR_PID 2>/dev/null || true
kill -"$SIG" $CCTR_PID 2>/dev/null || true

wait $CCTR_PID 2>/dev/null || true
sleep 0.3

echo "teardown_ran=$(test -f /tmp/cctr_sigdup_teardown && echo yes || echo no)"
echo "force_quit=$(grep -q 'Force quit' /tmp/cctr_sigdup_out && echo yes || echo no)"
echo "leftover_children=$(pgrep -f 'sleep 31337' | wc -l | tr -d ' ')"