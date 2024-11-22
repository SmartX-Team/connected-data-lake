# Copyright (c) 2024 Ho Kim (ho.kim@ulagbulag.io). All rights reserved.
# Use of this source code is governed by a GPL-3-style license that can be
# found in the LICENSE file.

# Load environment variables
set dotenv-load

# Configure environment variables
export OCI_BUILD_LOG_DIR := env_var_or_default('OCI_BUILD_LOG_DIR', './logs/')
export OCI_IMAGE_BASE := env_var_or_default('OCI_IMAGE_BASE', 'quay.io/ulagbulag')
export OCI_IMAGE_VERSION := env_var_or_default('OCI_IMAGE_VERSION', 'latest')
export OCI_PLATFORMS := env_var_or_default('OCI_PLATFORMS', 'linux/amd64')

export DEFAULT_CONTAINER_RUNTIME := env_var_or_default('CONTAINER_RUNTIME', 'docker buildx')
export DEFAULT_RUNTIME_PACKAGE := env_var_or_default('RUNTIME_PACKAGE', 'cdl')

default:
  @just run

init-conda:
  conda install --yes \
    -c pytorch -c nvidia \
    autopep8 pip python \
    pytorch torchvision torchaudio pytorch-cuda=11.8
  pip install -r ./requirements.txt

fmt:
  cargo fmt --all

build: fmt
  cargo build --all --workspace

clippy: fmt
  cargo clippy --all --workspace

test: clippy
  cargo test --all --workspace

run *ARGS:
  cargo run --package "${DEFAULT_RUNTIME_PACKAGE}" --release -- {{ ARGS }}

run-operator *ARGS:
  cargo run --package "cdl-k8s-operator" --release -- {{ ARGS }}

oci-build *ARGS:
  mkdir -p "${OCI_BUILD_LOG_DIR}/cdl"
  ${DEFAULT_CONTAINER_RUNTIME} build \
    --file './Dockerfile' \
    --tag "${OCI_IMAGE_BASE}/connected-data-lake:${OCI_IMAGE_VERSION}" \
    --build-arg PACKAGE="cdl" \
    --platform "${OCI_PLATFORMS}" \
    --pull \
    {{ ARGS }} \
    . 2>&1 | tee "${OCI_BUILD_LOG_DIR}/cdl/build-$( date -u +%s ).log"

oci-build-operator *ARGS:
  mkdir -p "${OCI_BUILD_LOG_DIR}/cdl-k8s-operator"
  ${DEFAULT_CONTAINER_RUNTIME} build \
    --file './Dockerfile' \
    --tag "${OCI_IMAGE_BASE}/connected-data-lake-operator:${OCI_IMAGE_VERSION}" \
    --build-arg PACKAGE="cdl-k8s-operator" \
    --platform "${OCI_PLATFORMS}" \
    --pull \
    {{ ARGS }} \
    . 2>&1 | tee "${OCI_BUILD_LOG_DIR}/cdl-k8s-operator/build-$( date -u +%s ).log"

oci-push: (oci-build "--push")

oci-push-operator: (oci-build-operator "--push")

oci-push-and-update-operator: oci-push-operator
  @just helm-clean-install

helm-clean-install:
  helm uninstall -n "cdl-operator" "cdl-operator" || true
  helm install -n "cdl-operator" "cdl-operator" "./charts/cdl-operator"
