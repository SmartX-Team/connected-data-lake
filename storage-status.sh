#!/bin/bash
# Copyright (c) 2023 Ho Kim (ho.kim@ulagbulag.io). All rights reserved.
# Use of this source code is governed by a GPL-3-style license that can be
# found in the LICENSE file.

# Prehibit errors
set -e -o pipefail

###########################################################
#   Main Function                                         #
###########################################################

echo '---'
for cluster_name in $("$(dirname "$0")/ceph-ls.sh"); do
    echo "${cluster_name}:"

    for node_name in $(
        kubectl get nodes \
            --context "${cluster_name}" \
            --output jsonpath \
            --selector 'node-role.kubernetes.io/kiss=Storage' \
            --template '{.items[*].metadata.name}'
    ); do
        echo "  ${node_name}:"

        echo "    Chassis: "
        kubectl run "storage-status-${node_name}" \
            --context "${cluster_name}" \
            --image 'docker.io/library/ubuntu:latest' \
            --overrides="{ \"spec\": { \"nodeSelector\": { \"kubernetes.io/hostname\": \"${node_name}\" } } }" \
            --privileged \
            --rm \
            --restart='Never' \
            --stdin \
            --tty \
            -- \
            bash -c 'apt-get update -q >dev/null && apt-get install -yq dmidecode >/dev/null && dmidecode' 2>/dev/null |
            grep -A3 '^System Information' |
            tail -n '+2'

        echo -n "    CPU: "
        kubectl run "storage-status-${node_name}" \
            --context "${cluster_name}" \
            --image 'docker.io/library/ubuntu:latest' \
            --overrides="{ \"spec\": { \"nodeSelector\": { \"kubernetes.io/hostname\": \"${node_name}\" } } }" \
            --privileged \
            --rm \
            --restart='Never' \
            --stdin \
            --tty \
            -- \
            cat /proc/cpuinfo |
            grep -Po '^model name[ \t]+\: +\K.*$' |
            head -n1 ||
            true

        echo -n "    Memory: "
        kubectl run "storage-status-${node_name}" \
            --context "${cluster_name}" \
            --image 'docker.io/library/ubuntu:latest' \
            --overrides="{ \"spec\": { \"nodeSelector\": { \"kubernetes.io/hostname\": \"${node_name}\" } } }" \
            --privileged \
            --rm \
            --restart='Never' \
            --stdin \
            --tty \
            -- \
            cat /proc/meminfo |
            grep -Po 'MemTotal\: +\K[0-9]+ +[a-zA-Z]+$'

        echo -n "    NIC: "
        kubectl run "storage-status-${node_name}" \
            --context "${cluster_name}" \
            --image 'docker.io/library/ubuntu:latest' \
            --overrides="{ \"spec\": { \"nodeSelector\": { \"kubernetes.io/hostname\": \"${node_name}\" } } }" \
            --privileged \
            --rm \
            --restart='Never' \
            --stdin \
            --tty \
            -- \
            bash -c 'apt-get update -q >dev/null && apt-get install -yq pciutils >/dev/null && for pci_id in $(lspci 2>/dev/null | grep "Ethernet controller\:" | grep "Mellanox" | awk '"'"'{print $1}'"'"'); do lspci -vv -s "${pci_id}" | grep -Po "Part number\: +\K[A-Z0-9-]+" | xargs echo -n ""; done' \
            2>/dev/null
        echo

        echo -n "    PCI-E: "
        kubectl run "storage-status-${node_name}" \
            --context "${cluster_name}" \
            --image 'docker.io/library/ubuntu:latest' \
            --overrides="{ \"spec\": { \"nodeSelector\": { \"kubernetes.io/hostname\": \"${node_name}\" } } }" \
            --privileged \
            --rm \
            --restart='Never' \
            --stdin \
            --tty \
            -- \
            bash -c 'apt-get update -q >dev/null && apt-get install -yq dmidecode >/dev/null && dmidecode' 2>/dev/null |
            grep -Po 'PCI Express [1-9]+' |
            sort |
            tail -n 1

        echo -n "    Storage: "
        kubectl run "storage-status-${node_name}" \
            --context "${cluster_name}" \
            --image 'docker.io/library/ubuntu:latest' \
            --overrides="{ \"spec\": { \"nodeSelector\": { \"kubernetes.io/hostname\": \"${node_name}\" } } }" \
            --privileged \
            --rm \
            --restart='Never' \
            --stdin \
            --tty \
            -- \
            lsblk -b -io KNAME,TYPE,SIZE |
            grep ' disk ' |
            grep -P '^[ns][dv]' |
            awk '{print $3}' |
            paste -sd+ |
            bc
    done
done
