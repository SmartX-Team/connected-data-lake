# Connected Data Lake operator Helm Chart

## Usage

```bash
# Register the Connected Data Lake repository
helm repo add cdl "https://smartx-team.github.io/connected-data-lake"

# Deploy a Connected Data Lake operator
NAME="cdl-operator"
helm install $NAME "cdl/cdl-operator" \
    --namespace "cdl-operator"
```

## Uninstall

```bash
NAME="cdl-operator"
helm uninstall $NAME --namespace "cdl-operator"
```
