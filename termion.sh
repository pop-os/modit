#!/usr/bin/env bash

set -ex

cargo fmt
cargo build --release --example termion
echo Press enter to launch editor
read
target/release/examples/termion src/lib.rs 2>termion.log
