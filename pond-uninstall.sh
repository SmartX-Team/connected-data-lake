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

    # Uninstall Data Pond
    kubectl delete modelstoragebindings \
        --all-namespaces \
        --context "${cluster_name}" \
        --selector 'dash.ulagbulag.io/kind=connected-data-lake'
    kubectl delete \
        --context "${cluster_name}" \
        --filename "$(pwd)/clusters/${cluster_name}.yaml" || true
done

# Cleanup
kubectl config use-context 'default'

# Done
exec echo 'Finished!'
