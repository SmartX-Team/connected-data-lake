#!/bin/bash
# Copyright (c) 2023 Ho Kim (ho.kim@ulagbulag.io). All rights reserved.
# Use of this source code is governed by a GPL-3-style license that can be
# found in the LICENSE file.

# Prehibit errors
set -e -o pipefail

###########################################################
#   Main Function                                         #
###########################################################

# Configure environment variables
export KUBECONFIG=~/.kube/config-docker-buildkit.yaml
if [ ! -f "${KUBECONFIG}" ]; then
    echo "Please configure your k8s cluster to bootstrap docker buildkit on: \"${KUBECONFIG}\"" >&2
    exit 1
fi

# Bootstrap
docker buildx create \
    --bootstrap \
    --driver kubernetes \
    --driver-opt='namespace=buildkit,"nodeselector=kubernetes.io/arch=amd64,node-role.kubernetes.io/kiss=Compute"' \
    --name kube \
    --node=builder-amd64 \
    --platform=linux/amd64 \
    --use
docker buildx create \
    --append \
    --bootstrap \
    --driver kubernetes \
    --driver-opt='namespace=buildkit,"nodeselector=kubernetes.io/arch=arm64,node-role.kubernetes.io/kiss=Compute"' \
    --name kube \
    --node=builder-arm64 \
    --platform=linux/arm64 \
    --use

# Completed
exec echo 'Finished!'
