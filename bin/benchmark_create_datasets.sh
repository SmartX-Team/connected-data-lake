#!/usr/bin/bash

for k in $(seq $@); do
    cargo run --package cdl-benchmark --release -- \
        --num-threads 20 \
        create dataset \
        --num-k $k
done
