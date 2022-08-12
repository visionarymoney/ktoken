#!/bin/bash
TARGET="${CARGO_TARGET_DIR:-target}"
set -e
cd "`dirname $0`"

cargo +stable build --all --target wasm32-unknown-unknown --release
cp $TARGET/wasm32-unknown-unknown/release/kt.wasm ./res/
cp $TARGET/wasm32-unknown-unknown/release/ft.wasm ./res/
