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
CONFIG_PATH_DEFAULT="$(pwd)/benchmark/config.yaml"
NAMESPACE_DEFAULT="tenant-e"

# Configure environment variables
CONFIG_PATH="${CONFIG_PATH:-$CONFIG_PATH_DEFAULT}"
NAMESPACE="${NAMESPACE:-$NAMESPACE_DEFAULT}"

###########################################################
#   Patch Function                                        #
###########################################################

function create_job() {
    filename="$1"

    # Inspect job
    job_name="$(
        kubectl \
            --context 'autodata-ai-compute-1' \
            apply \
            --filename "${filename}" \
            --dry-run='client' | awk '{print $1}'
    )"

    # Create job
    kubectl \
        --context 'autodata-ai-compute-1' \
        apply \
        --filename "${filename}" >/dev/null

    # Wait until job is ready
    pod_name="$(echo "${job_name}" | cut -d '/' -f 2)"
    kubectl wait pods \
        --context 'autodata-ai-compute-1' \
        --for=condition=Ready \
        --namespace "${NAMESPACE}" \
        --timeout '24h' \
        --selector "name=${pod_name}" >/dev/null

    echo "${job_name}"
}

function delete_job() {
    filename="$1"
    job_name="$2"

    # Delete job
    kubectl \
        --context 'autodata-ai-compute-1' \
        delete \
        --filename "${filename}" >/dev/null 2>/dev/null || true

    # Wait until pod is deleted
    pod_name="$(echo "${job_name}" | cut -d '/' -f 2)"
    while kubectl get pods \
        --context 'autodata-ai-compute-1' \
        --namespace "${NAMESPACE}" \
        --output name \
        --selector "name=${pod_name}" |
        grep '.' >/dev/null 2>/dev/null; do
        sleep 1
    done

    # Cleanup
    rm "${filename}"
}

function patch_env() {
    filename="$1"
    key="$2"
    value="$3"

    yq -i ".spec.template.spec.containers[].env[] | select(.name == \"${key}\") | .value |= \"${value}\" | parent | parent | parent | parent | parent | parent | parent | parent" "${filename}"
}

function wait_job() {
    job_name="$1"

    # Wait until job is completed
    kubectl wait \
        --context 'autodata-ai-compute-1' \
        --for=condition=complete \
        --namespace "${NAMESPACE}" \
        --timeout '24h' \
        "${job_name}" >/dev/null
}

###########################################################
#   Main Function                                         #
###########################################################

echo '----------------------------------------------------------------------------------------'
echo ' Receiver  DataSize  MessengerType PayloadSize  TotalMessages ModelOut                  '
echo '----------------------------------------------------------------------------------------'

for use_receiver in $(yq -r '.useReceiver[]' ./benchmark/config.yaml); do
    for messenger_type in $(yq -r '.messengerTypes[]' ./benchmark/config.yaml); do
        for data_size in $(yq -r '.dataSize[]' ./benchmark/config.yaml); do
            for payload_size in $(yq -r '.payloadSize[]' ./benchmark/config.yaml); do
                for total_messages in $(yq -r '.totalMessages[]' ./benchmark/config.yaml); do
                    # Resize total messages if payload size is given
                    if [ "x${payload_size}" != 'x0' ]; then
                        total_messages='10K'
                    fi

                    # Define sender model out
                    model_out='performance-test-src-sync'
                    if [ "x${use_receiver}" = 'xfalse' ]; then
                        if [ "x${messenger_type}" = 'xNats' ]; then
                            model_out='performance-test-src'
                        fi
                    fi

                    # Skip if Kafka with receiver
                    if [ "x${use_receiver}" = 'xtrue' ]; then
                        if [ "x${messenger_type}" = 'xKafka' ]; then
                            continue
                        fi
                    fi

                    # Show current metrics
                    echo -e " ${use_receiver}\t   ${data_size}\t     ${messenger_type}\t   ${payload_size}\t\t${total_messages}\t      ${model_out}"

                    if [ "x${use_receiver}" = 'xtrue' ]; then
                        # Copy benchmark script
                        receiver="/tmp/benchmark-receiver-$(date -u +%s).yaml"
                        cp "$(dirname "${CONFIG_PATH}")/receiver.yaml" "${receiver}"

                        # Patch script
                        patch_env "${receiver}" 'PIPE_DEFAULT_MESSENGER' "${messenger_type}"
                        patch_env "${receiver}" 'PIPE_PERFORMANCE_TEST_DATA_SIZE' "${data_size}"
                        patch_env "${receiver}" 'PIPE_PERFORMANCE_TEST_PAYLOAD_SIZE' "${payload_size}"
                        patch_env "${receiver}" 'PIPE_PERFORMANCE_TEST_TOTAL_MESSAGES' "${total_messages}"

                        # Execute job
                        receiver_job="$(create_job "${receiver}")"
                        sleep 3
                    fi

                    # Copy benchmark script
                    sender="/tmp/benchmark-sender-$(date -u +%s).yaml"
                    cp "$(dirname "${CONFIG_PATH}")/sender.yaml" "${sender}"

                    # Patch script
                    patch_env "${sender}" 'PIPE_DEFAULT_MESSENGER' "${messenger_type}"
                    patch_env "${sender}" 'PIPE_MODEL_OUT' "${model_out}"
                    patch_env "${sender}" 'PIPE_PERFORMANCE_TEST_DATA_SIZE' "${data_size}"
                    patch_env "${sender}" 'PIPE_PERFORMANCE_TEST_PAYLOAD_SIZE' "${payload_size}"
                    patch_env "${sender}" 'PIPE_PERFORMANCE_TEST_TOTAL_MESSAGES' "${total_messages}"

                    # Execute job
                    sender_job="$(create_job "${sender}")"

                    # Wait until job is completed
                    wait_job "${sender_job}"

                    # Cleanup
                    delete_job "${sender}" "${sender_job}"
                    if [ "x${use_receiver}" = 'xtrue' ]; then
                        delete_job "${receiver}" "${receiver_job}"
                    fi
                done
            done
        done
    done
done

# Done
exec echo 'Finished!'
