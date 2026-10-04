#!/bin/sh
set -eu

cargo test --lib
cargo check --offline

