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

    # Skip if no ceph cluster
    if
        ! kubectl \
            --context "${cluster_name}" \
            get namespace 'csi-rook-ceph' \
            >/dev/null 2>/dev/null
    then
        continue
    fi

    # Install MinIO Operator
    if
        ! kubectl \
            --context "${cluster_name}" \
            get namespace 'minio-operator' \
            >/dev/null 2>/dev/null
    then
        cluster_domain="$(
            kubectl \
                --context "${cluster_name}" \
                --namespace 'kube-system' \
                --output jsonpath \
                --template '{.data.Corefile}' \
                get configmap coredns |
                grep -Po '^ +kubernetes +\K[0-9a-z\.-]+'
        )"
        helm upgrade \
            --create-namespace \
            --install \
            --namespace 'minio-operator' \
            'minio-operator' "$(pwd)/charts/minio-operator-5.0.9.tgz" \
            --set 'operator.env[0].name=CLUSTER_DOMAIN' \
            --set "operator.env[0].value=${cluster_domain}"
    fi
done

# Cleanup
kubectl config use-context 'default'

# Done
exec echo 'Finished!'
