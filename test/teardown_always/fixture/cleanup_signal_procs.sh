#!/bin/bash
# Clear markers and any test processes left over from an earlier run.
#
# This lives in a script rather than inline in the corpus file because
# `pkill -f` matches against whole command lines, and `bash -c` stores the
# entire script it was given in its own argv. An inline `pkill -f 'sleep N'`
# therefore matches the very shell running it and kills itself, which is a
# non-zero exit on Linux. A script's argv is just its path, so there is
# nothing for the pattern to match.

rm -f /tmp/cctr_sigdup_* /tmp/cctr_slowtd_*
pkill -f 'sleep 31337' 2>/dev/null || true
exit 0