#!/usr/bin/bash

if [ -z "${SOURCE_BUCKET_ADDRESS}" ]; then
    echo 'Environment variable 'SOURCE_BUCKET_ADDRESS' not set' >&2
    exit
fi

for k in $(seq $@); do
    cargo run --package cdl-benchmark --release -- \
        --num-threads 20 \
        sync \
        --num-k $k
done
