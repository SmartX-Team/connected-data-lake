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

    # Parse cluster name
    export CLUSTER_NAME="$(
        kubectl -n kube-system get configmap coredns -o yaml |
            yq -r '.data.Corefile' |
            grep -Po ' +kubernetes \K[\w\.\_\-]+'
    )"

    # Do not use PVC if no ceph cluster
    if
        ! kubectl \
            --context "${cluster_name}" \
            get namespace 'csi-rook-ceph' \
            >/dev/null 2>/dev/null
    then
        export NATS_ENABLE_PVC="false"
    else
        export NATS_ENABLE_PVC="true"
    fi

    # Uninstall NATS
    helm uninstall --namespace 'nats-io' nats || true
    kubectl delete namespace 'nats-io'
done

# Cleanup
kubectl config use-context 'default'

# Done
exec echo 'Finished!'
