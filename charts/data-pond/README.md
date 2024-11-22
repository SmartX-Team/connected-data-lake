# Data Pond instance Helm Chart

## Usage

```bash
# Register the Connected Data Lake repository
helm repo add cdl "https://smartx-team.github.io/connected-data-lake"

# Deploy a Data Pond storage instance
NAME="my-storage-hoya-123"  # TODO: Change it to your unique app name
helm install $NAME "cdl/data-pond" \
    --namespace "name-twin"
```

## Uninstall

```bash
NAME="my-storage-hoya-123"  # TODO: Change it to your app name
helm uninstall $NAME --namespace "name-twin"
```
