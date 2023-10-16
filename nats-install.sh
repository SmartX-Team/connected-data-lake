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

###########################################################
#   Main Function                                         #
###########################################################

for cluster_name in $("$(dirname "$0")/ceph-ls.sh"); do
    echo -n "* ${cluster_name}: "
    kubectl config use-context "${cluster_name}"

    # Install NATS
    pushd "${OPENARK_HOME}/templates/messengers/nats/"
    ./install.sh
    popd
done

# Cleanup
kubectl config use-context 'default'

# Done
exec echo 'Finished!'
