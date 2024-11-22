# Connected Data Lake Dataset instance Helm Chart

## Usage

```bash
# Register the Connected Data Lake repository
helm repo add cdl "https://smartx-team.github.io/connected-data-lake"

# Deploy a Connected Data Lake Dataset instance
NAME="my-dataset-hoya-123"  # TODO: Change it to your unique app name
helm install $NAME "cdl/cdl-dataset" \
    --namespace "name-twin"
```

## Uninstall

```bash
NAME="my-dataset-hoya-123"  # TODO: Change it to your app name
helm uninstall $NAME --namespace "name-twin"
```
