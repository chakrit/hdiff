#!/bin/sh
set -eu
export CARGO_TARGET_DIR=/Users/chakrit/Documents/chakrit/hdiff/target
cargo fmt --check
cargo clippy --all-targets --locked -- -D warnings
cargo test --locked
if grep -rnE '^.{91,}$' src tests bench | grep -E '^[^:]+:[0-9]+:[[:space:]]*//'; then
  echo "comment over 90 columns"; exit 1
fi
echo "checks passed"
