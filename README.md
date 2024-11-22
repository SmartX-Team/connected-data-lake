# Connected Data Lake

## Usage

```bash
docker run --rm 'quay.io/ulagbulag/connected-data-lake:latest'
```

## Install K8S Operator

```bash
# Register the Connected Data Lake repository
helm repo add cdl "https://smartx-team.github.io/connected-data-lake"

# Deploy a Connected Data Lake operator
helm install -n "cdl-operator" "cdl-operator" "cdl/cdl-operator"
```

### Install Dependencies on Ubuntu 24.04

```bash
# Install os dependencies
sudo apt-get update
sudo apt-get install -y \
  build-essential \
  fuse \
  libfuse-dev \
  libprotoc-dev \
  protobuf-compiler \
  rustup

# Install & Update the latest stable rust
rustup default stable
```

## Build on the local machine

### Build Requirements

- gcc
- fuse
- protobuf
- rust >=1.82

### Build CDL rust CLI

```bash
cargo run --release --
```

### Build CDL python API

Please check your python virtual environment (i.e. conda) before running.

```bash
cd python
maturin develop --release
```

### Build K8S Operator

Please check your kubernetes config file `~/.kube/config` before running.

```bash
cargo run --package 'cdl-k8s-operator' --release --
```
