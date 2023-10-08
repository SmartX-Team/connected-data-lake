#!/bin/bash
# Copyright (c) 2023 Ho Kim (ho.kim@ulagbulag.io). All rights reserved.
# Use of this source code is governed by a GPL-3-style license that can be
# found in the LICENSE file.

# Prehibit errors
set -e -o pipefail

###########################################################
#   Main Function                                         #
###########################################################

for cluster_config_file in $(
    find "$(pwd)/clusters" \
        -maxdepth 1 \
        -mindepth 1 \
        -type f |
        sort
); do
    # Skip if not .yaml
    if
        ! echo "${cluster_config_file}" |
            grep -Po '\.yaml$' >/dev/null
    then
        continue
    fi

    # Skip if not regular file
    if
        ! echo "${cluster_config_file}" |
            grep -Po '^/(.+/)*\K[0-9a-z-]+\.yaml$' >/dev/null
    then
        continue
    fi

    echo "${cluster_config_file}" |
        grep -Po '^/(.+/)*\K[0-9a-z-]+'
done
