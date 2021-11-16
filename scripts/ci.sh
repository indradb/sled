#!/bin/bash

set -ex

rust_variant=$1
os=$2

cargo test

if [ "$os" == "ubuntu-latest" ]; then
    if [ "$rust_variant" == "stable" ]; then
        cargo clippy
        cargo fmt -- --check
    else
        cargo check --all-features
    fi
fi
