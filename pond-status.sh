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
    if
        kubectl \
            --context "${cluster_name}" \
            get namespace 'minio-operator' \
            >/dev/null 2>/dev/null
    then
        echo -e "\e[32m[ ${cluster_name} ]\e[0m"
        kubectl \
            --context "${cluster_name}" \
            get modelstorage \
            --all-namespaces
    fi
done
