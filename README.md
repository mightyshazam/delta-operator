# delta-operator

A kubernetes operator for [delta lake](https://delta.io) tables built on [delta-rs](https://github.com/delta-io/delta.rs). It allows for the creation and maintenance of delta tables via [kubernetes](https://k8s.io) custom resources.

## Quickstart and documentation

```sh
# Apply all set of production resources defined in kustomization.yaml in `production` directory .
kubectl apply -k https://github.com/mightyshazam/delta-operator/manifests/production
```

```yaml
apiVersion: delta-operator.rs/v1alpha1
kind: DeltaTable
metadata:
  name: azure-clowns
spec:
  name: clowns
  table_uri: az://tests/clowns
  allow_http: true
  partition_columns:
    - date
  checkpoint_configuration:
    criteria: Time
    time_interval: 5m
  optimize_configuration:
    criteria: Time
    time_interval: 24h
  vacuum_configuration:
    criteria: Time
    time_interval: 24h
  storage_options:
    azure_storage_allow_http: "true"
    azure_storage_account_name: "devstoreaccount1"
    azure_storage_account_key: "Eby8vdM02xNOcqFlqUwJPLlmEtlCDXJ1OUzFT50uSRZ6IFsuFq2UVErCz4I6tq/K1SZFPTOtr/KBHBeksoGMGw=="
    azure_container_name: "tests"
    azure_storage_use_emulator: "true"
    azure_allow_http: "true"
    azure_storage_connection_string: "DefaultEndpointsProtocol=http;AccountName=devstoreaccount1;AccountKey=Eby8vdM02xNOcqFlqUwJPLlmEtlCDXJ1OUzFT50uSRZ6IFsuFq2UVErCz4I6tq/K1SZFPTOtr/KBHBeksoGMGw==;BlobEndpoint=http://azurite.default:10000/devstoreaccount1;QueueEndpoint=http://azurite.default:10001/devstoreaccount1;"
  schema: |
    {
      "type": "struct",
      "fields": [
        {
          "name": "id",
          "type": "string",
          "nullable": true,
          "metadata": {}
        },
        {
          "name": "sender",
          "type": "string",
          "nullable": true,
          "metadata": {}
        },
        {
          "name": "recipient",
          "type": "string",
          "nullable": true,
          "metadata": {}
        },
        {
          "name": "timestamp",
          "type": "timestamp",
          "nullable": true,
          "metadata": {}
        },
        {
          "name": "date",
          "type": "string",
          "nullable": true,
          "metadata": {}
        }
      ]
    }
```

## Dev Requirements

1. Tilt
2. Azure CLI
3. AWS CLI
