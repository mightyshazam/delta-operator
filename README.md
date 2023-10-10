# delta-operator

A kubernetes operator for [delta lake](https://delta.io) tables built on [delta-rs](https://github.com/delta-io/delta.rs). It allows for the creation and maintenance of delta tables via [kubernetes](https://k8s.io) custom resources.

## Quickstart and documentation

```sh
# Apply the set of production resources defined in kustomization.yaml in `production` directory .
kubectl apply -k https://github.com/mightyshazam/delta-operator/manifests/production
```

```yaml
apiVersion: delta-operator.rs/v1alpha1
kind: DeltaTable
metadata:
  name: localstack-clowns
spec:
  name: clowns
  table_uri: s3://tests/clowns
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
    AWS_S3_LOCKING_PROVIDER: "dynamodb"
    AWS_REGION: us-east-2
    AWS_STORAGE_ALLOW_HTTP: "true"
    AWS_ENDPOINT_URL: http://localstack.default:4566
    AWS_ACCESS_KEY_ID: test
    AWS_SECRET_ACCESS_KEY: test
    DYNAMO_LOCK_TABLE_NAME: "locks"
    DYNAMO_LOCK_OWNER_NAME: "clowns"
    DYNAMO_LOCK_PARTITION_KEY_VALUE: clowns_s3_tests
    DYNAMO_LOCK_REFRESH_PERIOD_MILLIS: "100"
    DYNAMO_LOCK_ADDITIONAL_TIME_TO_WAIT_MILLIS: "100"
    DYNAMO_LOCK_LEASE_DURATION: "2"
    something: elses
  schema_settings:
    manage: false
    value: |
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

1. [Tilt](https://tilt.dev)
2. [Azure CLI](https://learn.microsoft.com/en-us/cli/azure/install-azure-cli)
3. [AWS CLI](https://docs.aws.amazon.com/cli/latest/userguide/getting-started-install.html)

Run `tilt up` to get started.
