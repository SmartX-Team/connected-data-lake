# Copyright (c) 2024 Ho Kim (ho.kim@ulagbulag.io). All rights reserved.
# Use of this source code is governed by a Apache-2.0 license that can be
# found in the LICENSE file.

# Configure environment variables
ARG DEBIAN_VERSION="bookworm"
ARG PACKAGE="cdl"

# Be ready for serving
FROM docker.io/library/debian:${DEBIAN_VERSION} AS server

# Server Configuration
WORKDIR /usr/local/bin
CMD [ "/bin/sh" ]

# Be ready for building
FROM docker.io/library/rust:1-${DEBIAN_VERSION} AS builder

# Load source files
ADD . /src
WORKDIR /src

# Build it!
ENV RUST_MIN_STACK=2097152
ARG PACKAGE
RUN \
    # Cache build outputs
    --mount=type=cache,target=/src/target \
    --mount=type=cache,target=/usr/local/cargo/registry \
    # Create an output directory
    mkdir /out \
    # Build
    && cargo build --package "${PACKAGE}" --release --workspace \
    && mv "./target/release/${PACKAGE}" /out \
    && mv ./LICENSE /LICENSE

# Copy executable files
FROM server
ARG PACKAGE
COPY --from=builder /out/* /usr/local/bin/
COPY --from=builder /LICENSE /usr/share/licenses/${PACKAGE}/LICENSE
