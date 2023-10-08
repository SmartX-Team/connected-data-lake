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

    # Uninstall MinIO Operator
    if
        kubectl \
            --context "${cluster_name}" \
            get namespace 'minio-operator' \
            >/dev/null 2>/dev/null
    then
        helm uninstall \
            --namespace 'minio-operator' \
            'minio-operator'
        kubectl \
            --context "${cluster_name}" \
            delete namespace 'minio-operator'
    fi
done

# Cleanup
kubectl config use-context 'default'

# Done
exec echo 'Finished!'
