#!/usr/bin/bash

for k in $(seq $@); do
    cargo run --package cdl-benchmark --release -- \
        --num-threads 20 \
        create pond \
        --num-k $k
done
