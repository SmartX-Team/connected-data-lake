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

if [ "x${SSH_KEYFILE_PATH}" = 'x' ]; then
    echo "Environment variable \"SSH_KEYFILE_PATH\" is not set."
    exit 1
fi

###########################################################
#   Main Function                                         #
###########################################################

function box_ssh() {
    local box_ip="$1"

    ssh \
        -i "${SSH_KEYFILE_PATH}" \
        -o 'LogLevel=ERROR' \
        -o 'StrictHostKeyChecking no' \
        -o 'UserKnownHostsFile=/dev/null' \
        "kiss@${box_ip}" ${@:2}
}

for cluster_name in $("$(dirname "$0")/ceph-ls.sh"); do
    echo -n '* '
    kubectl config use-context "${cluster_name}"

    # Uninstall Rook-Ceph
    if
        kubectl \
            --context "${cluster_name}" \
            get namespace 'csi-rook-ceph' \
            >/dev/null 2>/dev/null
    then
        pushd "${OPENARK_HOME}/templates/csi/rook-ceph/"
        ./uninstall.sh || true
        popd
    fi

    # Cleanup Disks
    for box_ip in $(
        kubectl get nodes \
            --context "${cluster_name}" \
            --output jsonpath \
            --template '{.items[?(@.status.conditions[-1].status == "True")].status.addresses[0].address}'
    ); do
        echo "  - Cleanup: ${box_ip}"
        for box_disk_uuid in $(
            box_ssh "${box_ip}" sudo lshw -class disk |
                grep -Po 'logical name\: +/dev/\K[0-9a-z]+' |
                sort
        ); do
            # Skip if used disk
            if
                box_ssh "${box_ip}" ls -l /dev/disk/by-partuuid/ |
                    grep "/${box_disk_uuid}" >/dev/null
            then
                continue
            fi

            # Skip if not physical disk
            if
                ! box_ssh "${box_ip}" ls -l /dev/disk/by-id/ |
                    grep "${box_disk_uuid}" >/dev/null
            then
                continue
            fi

            echo "    - /dev/${box_disk_uuid}"
            box_ssh "${box_ip}" sudo wipefs --all --force "/dev/${box_disk_uuid}" >/dev/null &&
                box_ssh "${box_ip}" sync || true
            box_ssh "${box_ip}" sudo sgdisk --zap-all "/dev/${box_disk_uuid}" >/dev/null &&
                box_ssh "${box_ip}" sync || true
            box_ssh "${box_ip}" sudo dd if='/dev/zero' of="/dev/${box_disk_uuid}" bs=1M count=1024 status=none &&
                box_ssh "${box_ip}" sync || true
            box_ssh "${box_ip}" sudo blkdiscard --force "/dev/${box_disk_uuid}" >/dev/null 2>/dev/null &&
                box_ssh "${box_ip}" sync || true
            box_ssh "${box_ip}" sudo partprobe "/dev/${box_disk_uuid}" >/dev/null &&
                box_ssh "${box_ip}" sync || true
        done
    done
done

# Cleanup
kubectl config use-context 'default'

# Done
exec echo 'Finished!'
