#!/bin/bash
# Copyright (c) 2023 Ho Kim (ho.kim@ulagbulag.io). All rights reserved.
# Use of this source code is governed by a GPL-3-style license that can be
# found in the LICENSE file.

# Prehibit errors
set -e -o pipefail

###########################################################
#   Main Function                                         #
###########################################################

for cluster_name in $("$(dirname "$0")/ceph-ls.sh"); do
    echo -n "* ${cluster_name}: "
    kubectl config use-context "${cluster_name}"

    # Infer namespace
    cluster_namespace="$(
        cat "$(pwd)/clusters/${cluster_name}.yaml" |
            grep -Po '^ +namespace\: *\K[0-9a-z-]+$' |
            head -n 1 ||
            true
    )"
    if [ "x${cluster_namespace}" = 'x' ]; then
        continue
    elif
        ! echo "${cluster_namespace}" |
            grep -Po '^tenant-[0-9a-z-]+' >/dev/null
    then
        continue
    fi

    # Install Data Pond
    kubectl apply \
        --context "${cluster_name}" \
        --filename "$(pwd)/clusters/${cluster_name}.yaml"
    cat "$(pwd)/clusters/_common.yaml" |
        sed "s/__NAMESPACE__/${cluster_namespace}/g" |
        kubectl create -f - || true

    # Bind models to each owned storage
    if
        kubectl \
            --context "${cluster_name}" \
            get namespace 'csi-rook-ceph' \
            >/dev/null 2>/dev/null
    then
        cat "$(pwd)/clusters/_binding.yaml" |
            sed "s/__NAMESPACE__/${cluster_namespace}/g" |
            kubectl create -f - || true
    fi

    # Install Data Pond Functions
    for function_set in $(ls "$(pwd)/functions"); do
        kubectl create configmap "${function_set}-functions" \
            --namespace="${cluster_namespace}" \
            --from-file=$(pwd)/functions/${function_set} \
            --output=yaml \
            --dry-run=client |
            kubectl apply -f -
    done
done

# Cleanup
kubectl config use-context 'default'

# Done
exec echo 'Finished!'
