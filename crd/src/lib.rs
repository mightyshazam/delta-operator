//! The shared custom resource library for delta operator functions
//!
//! Provides the [`maintenance`] module and [`DeltaTable`] [`CustomResource`] type
//!
use deltalake::{DeltaConfigKey, DeltaOps, DeltaTableBuilder, SchemaTypeStruct};
use k8s_openapi::api::batch::v1::Job;
use k8s_openapi::api::core::v1::{
    ConfigMap, Container, EnvVar, EnvVarSource, ObjectFieldSelector, PodTemplateSpec, Secret,
};

use kube::api::{Patch, PatchParams, PostParams};
use kube::core::{Duration, ObjectMeta};
use kube::runtime::events::{Event, EventType, Recorder};
use kube::{runtime::controller::Action, CustomResource};
use kube::{Api, Client, Resource, ResourceExt};
use maintenance::JobSettings;
use schemars::JsonSchema;
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::collections::{BTreeMap, HashMap};
use std::fmt::Display;
use std::time::Duration as StdDuration;

pub mod maintenance;
pub type DeltaLakeTable = deltalake::DeltaTable;

const AZURITE_BLOB_STORAGE_URL: &str = "AZURITE_BLOB_STORAGE_URL";
macro_rules! compare_optional_interval {
    // match something(q,r,t,6,7,8) etc
    // compiler extracts function name and arguments. It injects the values in respective varibles.
    ($a:expr, $b:expr, $c:ty, $d:expr) => {
        $a >= $b.map(|i| i.into()).unwrap_or($d)
    };
}

macro_rules! match_maintenance_critiera_time {
    ($timestamp_field:expr, $now:expr, $interval_option:expr, $default_value:expr) => {
        match $timestamp_field {
            Some(last_timestamp) => {
                compare_optional_interval!(
                    StdDuration::from_secs(($now - last_timestamp) as u64),
                    $interval_option,
                    StdDuration,
                    $default_value
                )
            }
            None => true,
        }
    };
}

macro_rules! match_maintenance_critiera_commit {
    ($commit_field:expr, $current_commit:expr, $interval_option:expr, $default_value:expr) => {
        match $commit_field {
            Some(last_commit) => {
                compare_optional_interval!(
                    $current_commit - last_commit,
                    $interval_option,
                    i64,
                    $default_value
                )
            }
            None => true,
        }
    };
}

macro_rules! match_maintenance_criteria {
    ($criteria_option:expr, $timestamp_field:expr, $time_interval_option:expr, $default_time:expr, $commit_field:expr, $current_commit:expr, $commit_option:expr, $default_commit:expr) => {{
        let now = chrono::Utc::now();
        let timestamp_now = now.timestamp();
        match $criteria_option
            .as_ref()
            .unwrap_or(&MaintenanceCriteria::Time)
        {
            MaintenanceCriteria::Time => {
                match_maintenance_critiera_time!(
                    $timestamp_field,
                    timestamp_now,
                    $time_interval_option,
                    $default_time
                )
            }
            MaintenanceCriteria::Commit => match_maintenance_critiera_commit!(
                $commit_field,
                $current_commit,
                $commit_option,
                $default_commit
            ),
            MaintenanceCriteria::TimeAndCommit => {
                let time_match = match_maintenance_critiera_time!(
                    $timestamp_field,
                    timestamp_now,
                    $time_interval_option,
                    $default_time
                );
                let commit_match = match_maintenance_critiera_commit!(
                    $commit_field,
                    $current_commit,
                    $commit_option,
                    $default_commit
                );

                time_match || commit_match
            }
        }
    }};
}

pub const ANNOTATION_RECONCILIATION_POLICY: &str = "delta-operator.rs/reconciliation-policy";
pub const RECONCILIATION_MANAGE: &str = "manage";
pub const RECONCILIATION_DETACH: &str = "detach";

// 30 days
static DEFAULT_VACUUM_INTERVAL: StdDuration = StdDuration::from_secs(2_592_000);
// 24 hours
static DEFAULT_CHECKPOINT_INTERVAL: StdDuration = StdDuration::from_secs(86_400);
// 24 hours
static DEFAULT_OPTIMIZE_INTERVAL: StdDuration = StdDuration::from_secs(86_400);

#[derive(thiserror::Error, Debug)]
pub enum Error {
    #[error("Kubernetes error: {source}")]
    Kubernetes {
        #[from]
        source: kube::Error,
    },
    #[error("Deltalake error: {source}")]
    DeltaLake {
        #[from]
        source: deltalake::DeltaTableError,
    },
    #[error("Delta protocol error: {source}")]
    DeltaProtocol {
        #[from]
        source: deltalake::protocol::ProtocolError,
    },
    #[error("Missing reference {kind}/{name} for object {object}")]
    MissingReference {
        object: String,
        kind: ReferenceKind,
        name: String,
    },
    #[error("Invalid schema json: {source}")]
    SchemaJson {
        #[from]
        source: serde_json::Error,
    },
}

#[derive(Deserialize, Serialize, Clone, Debug, JsonSchema)]
pub enum ReferenceKind {
    ConfigMap,
    Secret,
}

#[derive(Deserialize, Serialize, Clone, Debug, JsonSchema)]
pub enum MaintenanceCriteria {
    Time,
    Commit,
    TimeAndCommit,
}

impl Display for ReferenceKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ReferenceKind::ConfigMap => write!(f, "ConfigMap"),
            ReferenceKind::Secret => write!(f, "Secret"),
        }
    }
}

#[derive(CustomResource, Deserialize, Serialize, Clone, Debug, JsonSchema)]
#[cfg_attr(test, derive(Default))]
#[kube(
    kind = "DeltaTable",
    group = "delta-operator.rs",
    version = "v1alpha1",
    namespaced
)]
#[kube(status = "DeltaTableStatus", shortname = "dt")]

pub struct DeltaTableSpec {
    /// Name of the table
    pub name: String,
    /// Location of the table
    pub table_uri: String,
    /// Allow http uris
    pub allow_http: Option<bool>,
    /// Serialized json of delta table schema
    pub schema: String,
    /// Columns to use when partitioning the table
    pub partition_columns: Vec<String>,
    /// Configuration for checkpoints
    pub checkpoint_configuration: Option<DeltaTableMaintenceConfiguration>,
    /// Configuration for optimize actions
    pub optimize_configuration: Option<DeltaTableMaintenceConfiguration>,
    /// Configuration for vacuum actions
    pub vacuum_configuration: Option<DeltaTableVacuumConfiguration>,
    /// Set options used to initialize storage backend
    ///
    /// Options may be passed in the HashMap or set as environment variables. See documentation of
    /// underlying object store implementation for details.
    ///
    /// - [Azure options](https://docs.rs/object_store/latest/object_store/azure/enum.AzureConfigKey.html#variants)
    /// - [S3 options](https://docs.rs/object_store/latest/object_store/aws/enum.AmazonS3ConfigKey.html#variants)
    /// - [Google options](https://docs.rs/object_store/latest/object_store/gcp/enum.GoogleConfigKey.html#variants)
    pub storage_options: Option<BTreeMap<String, String>>,
    /// Like `storage_options`, but can come from [`ConfigMap`] or [`Secret`] resources
    pub storage_options_from: Option<Vec<StorageOptionReference>>,
    /// Delta table configuration settings
    pub configuration: Option<DeltaTableConfiguration>,
}

#[derive(Deserialize, Serialize, Clone, Debug, JsonSchema)]
pub struct StorageOptionReference {
    pub kind: ReferenceKind,
    pub name: String,
    pub optional: Option<bool>,
}

/// Represents settings for the delta table
/// For any properties not handled by the static properties, `additional_settings` allows
/// adding the options as key-value pairs.
/// The available properties can be found at the following links.
/// <https://docs.delta.io/latest/table-properties.html#delta-table-properties-reference>
/// <https://learn.microsoft.com/en-us/azure/databricks/delta/table-properties>
#[derive(Deserialize, Serialize, Clone, Debug, JsonSchema)]
pub struct DeltaTableConfiguration {
    pub enable_change_feed: Option<bool>,
    pub additional_settings: Option<HashMap<String, String>>,
}

#[derive(Deserialize, Serialize, Clone, Debug, JsonSchema)]
pub struct DeltaTableMaintenceConfiguration {
    pub time_interval: Option<Duration>,
    pub commit_interval: Option<i32>,
    pub disable: Option<bool>,
    pub criteria: Option<MaintenanceCriteria>,
}

#[derive(Deserialize, Serialize, Clone, Debug, JsonSchema)]
pub struct DeltaTableVacuumConfiguration {
    pub time_interval: Option<Duration>,
    pub commit_interval: Option<i32>,
    pub disable: Option<bool>,
    pub retention_period: Option<Duration>,
    pub criteria: Option<MaintenanceCriteria>,
}

/// The status object of `DeltaTable`
#[derive(Deserialize, Serialize, Clone, Default, Debug, JsonSchema)]
pub struct DeltaTableStatus {
    pub table_uri: String,
    pub schema: String,
    pub last_checkpoint_commit: Option<i64>,
    pub last_checkpoint_timestamp: Option<i64>,
    pub last_vacuum_commit: Option<i64>,
    pub last_vacuum_timestamp: Option<i64>,
    pub last_optimize_commit: Option<i64>,
    pub last_optimize_timestamp: Option<i64>,
    pub version: Option<i64>,
    pub is_healthy: Option<bool>,
}

impl DeltaTable {
    pub async fn delta_lake_table(&self, client: Client) -> Result<DeltaLakeTable, Error> {
        let namespace = self.metadata.namespace.as_ref().unwrap();
        let storage_options = self
            .accumulate_storage_options(client.clone(), namespace)
            .await?;
        self.create_delta_table(storage_options, false).await
    }

    fn extract_config(&self) -> Vec<(String, Option<String>)> {
        match self.spec.configuration.as_ref() {
            None => Vec::new(),
            Some(configuration) => {
                let mut data = match configuration.additional_settings.as_ref() {
                    None => HashMap::<String, String>::new(),
                    Some(additional_settings) => additional_settings.clone(),
                };

                if let Some(enable_change_feed) = configuration.enable_change_feed.as_ref() {
                    data.insert(
                        DeltaConfigKey::EnableChangeDataFeed.as_ref().to_owned(),
                        enable_change_feed.to_string(),
                    );
                }

                data.into_iter()
                    .map(|(key, value)| (key, Some(value)))
                    .collect::<Vec<(String, Option<String>)>>()
            }
        }
    }

    async fn create_delta_table(
        &self,
        storage_options: HashMap<String, String>,
        create_if_not_found: bool,
    ) -> Result<DeltaLakeTable, Error> {
        let mut table = DeltaTableBuilder::from_uri(&self.spec.table_uri)
            .with_allow_http(self.spec.allow_http.unwrap_or_default())
            .with_storage_options(storage_options)
            .build()?;

        match table.load().await {
            Ok(_) => Ok(table),
            Err(deltalake::DeltaTableError::NotATable(_)) if create_if_not_found => {
                let schema: SchemaTypeStruct = serde_json::de::from_str(&self.spec.schema)?;
                let columns = schema.get_fields().to_owned();
                Ok(DeltaOps(table)
                    .create()
                    .with_table_name(self.spec.name.clone())
                    .with_columns(columns)
                    .with_configuration(self.extract_config())
                    .with_partition_columns(self.spec.partition_columns.clone())
                    .with_metadata(serde_json::Map::new())
                    .await?)
            }
            Err(err) => Err(Error::DeltaLake { source: err }),
        }
    }

    pub async fn reconcile(
        &self,
        client: Client,
        _: Recorder,
        settings: &JobSettings,
    ) -> Result<Action, Error> {
        let namespace = &self.namespace().unwrap();
        let api: Api<DeltaTable> = Api::namespaced(client.clone(), namespace);
        if self.status.as_ref().is_none() {
            let pp = PostParams::default();
            let name = self.name_any();
            let mut o = api.get_status(&name).await?;
            o.status = Some(DeltaTableStatus::default());
            let status = serde_json::ser::to_vec(&o).unwrap();
            if let Err(e) = api
                .replace_status(self.meta().name.as_ref().unwrap(), &pp, status)
                .await
            {
                tracing::error!("failed to set status: {}", e);
            }
        }
        let storage_options = self
            .accumulate_storage_options(client.clone(), namespace)
            .await?;
        let table = match self.create_delta_table(storage_options.clone(), true).await {
            Ok(table) => table,
            Err(e) => {
                self.update_status_ok(
                    &api,
                    json!({
                    "table_uri": "",
                    "schema": "",
                    "is_healthy": false,
                        }),
                )
                .await;
                return Err(e);
            }
        };
        if self.requires_vacuum(&table) {
            self.create_job(
                client.clone(),
                maintenance::Action::Vacuum,
                namespace,
                settings,
                &storage_options,
            )
            .await?;
        }

        if self.requires_checkpoint(&table) {
            self.create_job(
                client.clone(),
                maintenance::Action::Checkpoint,
                namespace,
                settings,
                &storage_options,
            )
            .await?;
        }

        if self.requires_optimize(&table) {
            self.create_job(
                client.clone(),
                maintenance::Action::Optimize,
                namespace,
                settings,
                &storage_options,
            )
            .await?;
        }

        self.update_status_ok(
            &api,
            json!({
                        "version": table.version(),
                        "table_uri": "",
                        "schema": "",
            "is_healthy": true,
                    }),
        )
        .await;
        Ok(Action::await_change())
    }

    async fn update_status_ok(&self, api: &Api<DeltaTable>, value: serde_json::value::Value) {
        let pp = PatchParams::default();
        let api_version = Self::api_version(&()).to_string();
        let kind = Self::kind(&()).to_string();
        if let Err(e) = api
            .patch_status(
                self.meta().name.as_ref().unwrap(),
                &pp,
                &Patch::Merge(json!({
                    "apiVersion": api_version,
                    "kind": kind,
                    "status": value,
                })),
            )
            .await
        {
            tracing::error!("failed to patch delta table: {}", e);
        }
    }

    pub async fn cleanup(&self, _: Client, recorder: Recorder) -> Result<Action, Error> {
        // Document doesn't have any real cleanup, so we just publish an event
        let reconciliation_policy = match self.annotations().get(ANNOTATION_RECONCILIATION_POLICY) {
            Some(policy) => match policy.as_str() {
                RECONCILIATION_DETACH | RECONCILIATION_MANAGE => policy,
                _ => RECONCILIATION_MANAGE,
            },
            None => RECONCILIATION_MANAGE,
        };

        if reconciliation_policy == RECONCILIATION_DETACH {
            recorder
                .publish(Event {
                    type_: EventType::Normal,
                    reason: "DeleteRequested".into(),
                    note: Some(format!(
                        "Delete `{}`: no action due to policy",
                        self.name_any()
                    )),
                    action: "Deleting".into(),
                    secondary: None,
                })
                .await?;
            return Ok(Action::await_change());
        }

        recorder
            .publish(Event {
                type_: EventType::Normal,
                reason: "DeleteRequested".into(),
                note: Some(format!(
                    "Delete `{}`: delete not implemented",
                    self.name_any()
                )),
                action: "Deleting".into(),
                secondary: None,
            })
            .await?;
        Ok(Action::await_change())
    }

    async fn create_job(
        &self,
        client: Client,
        action: maintenance::Action,
        namespace: &str,
        settings: &JobSettings,
        storage_options: &HashMap<String, String>,
    ) -> Result<(), Error> {
        let api: Api<Job> = Api::namespaced(client, &settings.namespace);
        let pp = kube::api::PostParams {
            dry_run: false,
            field_manager: None,
        };
        /*let object_ref = self.object_ref(&());
        let owner = OwnerReference {
            api_version: object_ref.api_version.unwrap_or_default(),
            block_owner_deletion: Some(true),
            controller: Some(false),
            kind: object_ref.kind.unwrap_or_default(),
            name: object_ref.name.unwrap_or_default(),
            uid: object_ref.uid.unwrap_or_default(),
        };*/
        let mut env = vec![EnvVar {
            name: maintenance::ENV_WORKER_POD_NAME.into(),
            value: None,
            value_from: Some(EnvVarSource {
                field_ref: Some(ObjectFieldSelector {
                    api_version: None,
                    field_path: "metadata.name".into(),
                }),
                ..Default::default()
            }),
        }];

        if let Some((_, value)) = storage_options
            .iter()
            .find(|x| x.0.to_lowercase() == "azure_storage_use_emulator")
        {
            if let "false" | "0" = value.to_lowercase().as_str() {
            } else if let Ok(url) = std::env::var(AZURITE_BLOB_STORAGE_URL) {
                env.push(EnvVar {
                    name: AZURITE_BLOB_STORAGE_URL.into(),
                    value: Some(url),
                    value_from: None,
                })
            }
        };

        match api
            .create(
                &pp,
                &Job {
                    metadata: kube::core::ObjectMeta {
                        annotations: Some(settings.annotations.clone()),
                        labels: Some(settings.labels.clone()),
                        name: Some(format!("{}-{}", self.name_any(), action).to_lowercase()),
                        namespace: Some(settings.namespace.clone()),
                        //                        owner_references: Some(vec![owner]),
                        ..Default::default()
                    },
                    spec: Some(k8s_openapi::api::batch::v1::JobSpec {
                        template: PodTemplateSpec {
                            metadata: Some(ObjectMeta {
                                annotations: Some(settings.annotations.clone()),
                                labels: Some(settings.labels.clone()),
                                ..Default::default()
                            }),
                            spec: Some(k8s_openapi::api::core::v1::PodSpec {
                                containers: vec![Container {
                                    args: Some(vec![
                                        "--table".into(),
                                        self.name_unchecked(),
                                        "--namespace".into(),
                                        namespace.to_owned(),
                                        "--action".into(),
                                        action.to_string().to_lowercase(),
                                        "--worker-name".into(),
                                        "delta-maintenance-worker".into(),
                                    ]),
                                    env: Some(env),
                                    image: Some(settings.image.clone()),
                                    name: "maintenance".into(),
                                    resources: settings.resource_requirements.clone(),
                                    ..Default::default()
                                }],
                                restart_policy: Some("Never".into()),
                                service_account_name: Some(settings.service_account.clone()),
                                ..Default::default()
                            }),
                        },
                        ttl_seconds_after_finished: Some(60),
                        ..Default::default()
                    }),
                    ..Default::default()
                },
            )
            .await
        {
            Ok(_) => Ok(()),
            Err(e) => match e {
                kube::Error::Api(r) if r.code == 409 => Ok(()),
                _ => Err(e.into()),
            },
        }
    }

    fn requires_vacuum(&self, table: &DeltaLakeTable) -> bool {
        if let Some(vacuum_configuration) = &self.spec.vacuum_configuration {
            if let Some(true) = vacuum_configuration.disable {
                return false;
            }

            let default_status = DeltaTableStatus::default();
            let status = self.status.as_ref().unwrap_or(&default_status);
            return match_maintenance_criteria!(
                vacuum_configuration.criteria,
                status.last_vacuum_timestamp,
                vacuum_configuration.time_interval,
                DEFAULT_VACUUM_INTERVAL,
                status.last_vacuum_commit,
                table.version(),
                vacuum_configuration.commit_interval,
                100
            );
        }

        false
    }

    fn requires_optimize(&self, table: &DeltaLakeTable) -> bool {
        if let Some(config) = &self.spec.optimize_configuration {
            if let Some(true) = config.disable {
                return false;
            }

            let default_status = DeltaTableStatus::default();
            let status = self.status.as_ref().unwrap_or(&default_status);
            return match_maintenance_criteria!(
                config.criteria,
                status.last_optimize_timestamp,
                config.time_interval,
                DEFAULT_OPTIMIZE_INTERVAL,
                status.last_optimize_commit,
                table.version(),
                config.commit_interval,
                50
            );
        }

        false
    }

    fn requires_checkpoint(&self, table: &DeltaLakeTable) -> bool {
        if let Some(config) = &self.spec.checkpoint_configuration {
            if let Some(true) = config.disable {
                return false;
            }

            let default_status = DeltaTableStatus::default();
            let status = self.status.as_ref().unwrap_or(&default_status);
            return match_maintenance_criteria!(
                config.criteria,
                status.last_checkpoint_timestamp,
                config.time_interval,
                DEFAULT_CHECKPOINT_INTERVAL,
                status.last_checkpoint_commit,
                table.version(),
                config.commit_interval,
                10
            );
        }

        false
    }

    async fn accumulate_storage_options(
        &self,
        client: Client,
        namespace: &str,
    ) -> Result<HashMap<String, String>, Error> {
        let mut storage_options: HashMap<String, String> = HashMap::new();
        if let Some(options) = self.spec.storage_options.as_ref() {
            for option in options {
                storage_options.insert(option.0.clone(), option.1.clone());
            }
        }

        if let Some(references) = self.spec.storage_options_from.as_ref() {
            let config_api: Api<ConfigMap> = Api::namespaced(client.clone(), namespace);
            let secret_api: Api<Secret> = Api::namespaced(client.clone(), namespace);
            let mut config_maps: HashMap<&str, ConfigMap> = HashMap::new();
            let mut secrets_maps: HashMap<&str, Secret> = HashMap::new();
            for entry in references {
                let optional = entry.optional.unwrap_or_default();
                match entry.kind {
                    ReferenceKind::ConfigMap => {
                        self.get_reference_or_not(
                            &entry.name,
                            &config_api,
                            &mut config_maps,
                            optional,
                        )
                        .await?;
                        if let Some(content) =
                            &config_maps.get(entry.name.as_str()).as_ref().unwrap().data
                        {
                            for (k, v) in content {
                                storage_options.insert(k.clone(), v.clone());
                            }
                        }
                    }
                    ReferenceKind::Secret => {
                        self.get_reference_or_not(
                            &entry.name,
                            &secret_api,
                            &mut secrets_maps,
                            optional,
                        )
                        .await?;
                        if let Some(content) =
                            &secrets_maps.get(entry.name.as_str()).as_ref().unwrap().data
                        {
                            for (k, v) in content {
                                storage_options
                                    .insert(k.clone(), String::from_utf8_lossy(&v.0).into_owned());
                            }
                        }
                    }
                };
            }
        }

        Ok(storage_options)
    }

    async fn get_reference_or_not<'a, 'b, T: Clone + DeserializeOwned + std::fmt::Debug>(
        &self,
        entry: &'a str,
        api: &Api<T>,
        map: &'b mut HashMap<&'a str, T>,
        optional: bool,
    ) -> Result<(), Error>
    where
        'a: 'b,
    {
        if map.contains_key(entry) {
            return Ok(());
        }

        match api.get_opt(entry).await? {
            Some(config_map) => {
                map.insert(entry, config_map);
                Ok(())
            }
            None => {
                if optional {
                    Ok(())
                } else {
                    Err(Error::MissingReference {
                        object: self.name_any(),
                        kind: ReferenceKind::ConfigMap,
                        name: entry.to_owned(),
                    })
                }
            }
        }
    }
}
