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

    # Uninstall Data Pond Pipelines
    kubectl delete clusterissuers \
        --all \
        --context "${cluster_name}" \
        --namespace "${cluster_namespace}" || true
    kubectl delete certificates \
        --all \
        --context "${cluster_name}" \
        --namespace "${cluster_namespace}" || true
    kubectl delete issuers \
        --all \
        --context "${cluster_name}" \
        --namespace "${cluster_namespace}" || true
    kubectl delete natscluster \
        --all \
        --context "${cluster_name}" \
        --namespace "${cluster_namespace}" || true
    kubectl delete natsserviceroles \
        --all \
        --context "${cluster_name}" \
        --namespace "${cluster_namespace}" || true
    kubectl delete natsserviceroles \
        --all \
        --context "${cluster_name}" \
        --namespace "${cluster_namespace}" || true
    kubectl delete pods \
        --context "${cluster_name}" \
        --namespace "${cluster_namespace}" \
        --selector 'serviceType=pipe' || true
done

# Cleanup
kubectl config use-context 'default'

# Done
exec echo 'Finished!'
