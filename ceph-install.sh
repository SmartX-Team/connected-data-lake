#!/bin/bash
# Copyright (c) 2023 Ho Kim (ho.kim@ulagbulag.io). All rights reserved.
# Use of this source code is governed by a GPL-3-style license that can be
# found in the LICENSE file.

# Prehibit errors
set -e -o pipefail

###########################################################
#   Configuration                                         #
###########################################################

# Configure default environment variables
OPENARK_HOME_DEFAULT="${HOME}/Library/ipis-dev-env/netai-cloud"

# Configure environment variables
OPENARK_HOME="${OPENARK_HOME:-$OPENARK_HOME_DEFAULT}"

###########################################################
#   Main Function                                         #
###########################################################

for cluster_name in $("$(dirname "$0")/ceph-ls.sh"); do
    echo -n "* ${cluster_name}: "
    kubectl config use-context "${cluster_name}"

    # Count storage nodes
    cluster_storage_nodes_count="$(
        kubectl get nodes \
            --context "${cluster_name}" \
            --output jsonpath \
            --template '{.items[?(@.metadata.labels.node-role\.kubernetes\.io/kiss == "Storage")].metadata.name}' |
            wc -w
    )"

    # Skip if no storage nodes
    if [ "x${cluster_storage_nodes_count}" = 'x0' ]; then
        continue
    fi

    # Install Rook-Ceph
    if
        ! kubectl \
            --context "${cluster_name}" \
            get namespace 'csi-rook-ceph' \
            >/dev/null 2>/dev/null
    then
        pushd "${OPENARK_HOME}/templates/csi/rook-ceph/"
        sed -i "s/\(size\: \)[0-9]\+/\1${cluster_storage_nodes_count}/g" './values-cluster.yaml'
        ./install.sh
        popd
    fi
done

# Cleanup
kubectl config use-context 'default'

# Done
exec echo 'Finished!'
