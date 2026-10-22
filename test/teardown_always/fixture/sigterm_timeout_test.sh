#!/bin/bash
# Run cctr under `timeout -s TERM`, which signals the process group AND the
# monitored child, delivering the same SIGTERM twice. A single timeout must
# still be treated as one shutdown request, so teardown has to run.

FIXTURE_DIR="$1"
rm -f /tmp/cctr_sigdup_setup /tmp/cctr_sigdup_teardown /tmp/cctr_sigdup_started

timeout -s TERM 3 cctr "$FIXTURE_DIR/signal_dup" --no-color > /tmp/cctr_sigdup_out 2>&1

echo "setup_ran=$(test -f /tmp/cctr_sigdup_setup && echo yes || echo no)"
echo "test_started=$(test -f /tmp/cctr_sigdup_started && echo yes || echo no)"
echo "teardown_ran=$(test -f /tmp/cctr_sigdup_teardown && echo yes || echo no)"
echo "force_quit=$(grep -q 'Force quit' /tmp/cctr_sigdup_out && echo yes || echo no)"