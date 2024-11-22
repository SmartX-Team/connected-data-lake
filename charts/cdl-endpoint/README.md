# Connected Data Lake instance Helm Chart

## Usage

```bash
# Register the Connected Data Lake repository
helm repo add cdl "https://smartx-team.github.io/connected-data-lake"

# Deploy a Connected Data Lake endpoint instance
NAME="object-storage"  # NOTE: the name is fixed per namespace!
helm install $NAME "cdl/cdl-endpoint" \
    --namespace "name-twin"
```

## Uninstall

```bash
NAME="object-storage"  # NOTE: the name is fixed per namespace!
helm uninstall $NAME --namespace "name-twin"
```
