#!/usr/bin/env bash
set -eu
while [ "$1" != --output ]; do shift; done
case "$MOCK_KIND" in
  http) exit 22 ;;
  html) printf '<html>gateway timeout</html>' > "$2" ;;
  valid) cp "$MOCK_ARCHIVE" "$2" ;;
esac
