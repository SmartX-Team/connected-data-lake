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
            get namespace 'csi-rook-ceph' \
            >/dev/null 2>/dev/null
    then
        echo "${cluster_name}:"
        kubectl \
            --context "${cluster_name}" \
            --namespace 'csi-rook-ceph' \
            delete pods \
            --selector app='rook-ceph-operator'
        kubectl \
            --context "${cluster_name}" \
            --namespace 'csi-rook-ceph' \
            delete pods \
            --selector app='rook-ceph-osd' \
            --field-selector 'status.phase!=Running' ||
            true
    fi
done
