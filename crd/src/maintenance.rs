//! A collection of maintenance settings and functions for delta table maintenance
use std::{collections::BTreeMap, fmt::Display};

use crate::{DeltaLakeTable, DeltaTable, Error};
use clap::ValueEnum;
use deltalake::{DeltaOps, SchemaTypeStruct};
use k8s_openapi::api::core::v1::ResourceRequirements;
use kube::ResourceExt;
use serde::{Deserialize, Serialize};

pub const ENV_WORKER_POD_NAME: &str = "CONTROLLER_POD_NAME";

/// Represents settings for a maintenance job
pub struct JobSettings {
    pub namespace: String,
    pub image: String,
    pub labels: BTreeMap<String, String>,
    pub annotations: BTreeMap<String, String>,
    pub service_account: String,
    pub resource_requirements: Option<ResourceRequirements>,
}

/// Action options for maintenance
#[derive(Deserialize, Serialize, Clone, Debug, ValueEnum, PartialEq)]
pub enum Action {
    Checkpoint,
    Optimize,
    Vacuum,
}

impl Display for Action {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Action::Checkpoint => write!(f, "Checkpoint"),
            Action::Optimize => write!(f, "Optimize"),
            Action::Vacuum => write!(f, "Vacuum"),
        }
    }
}

/// Performs an optimize action on the specified [`deltalake::DeltaTable`] resource
/// using the [`DeltaTable`] provided to access it
pub async fn optimize_table(
    doc: &DeltaTable,
    table: DeltaLakeTable,
) -> Result<DeltaLakeTable, Error> {
    let result = DeltaOps(table).optimize().await?;
    metrics::increment_counter!(
        "optimize_executed_count",
        "table" => doc.name_any(),
        "namespace" => doc.namespace().unwrap()
    );
    Ok(result.0)
}

/// Performs a checkpoint action on the specified [`deltalake::DeltaTable`] resource
/// using the [`DeltaTable`] provided to access it
pub async fn checkpoint_table(
    doc: &DeltaTable,
    table: DeltaLakeTable,
) -> Result<DeltaLakeTable, Error> {
    deltalake::checkpoints::create_checkpoint(&table).await?;
    deltalake::checkpoints::cleanup_metadata(&table).await?;
    metrics::increment_counter!(
        "checkpoint_executed_count",
        "table" => doc.name_any(),
        "namespace" => doc.namespace().unwrap()
    );
    Ok(table)
}

/// Performs a vacuum action on the specified [`deltalake::DeltaTable`] resource
/// using the [`DeltaTable`] provided to access it
pub async fn vacuum_table(
    doc: &DeltaTable,
    table: DeltaLakeTable,
) -> Result<DeltaLakeTable, Error> {
    let mut vacuum = DeltaOps(table).vacuum();
    if let Some(schedule) = doc.spec.vacuum_configuration.as_ref() {
        if let Some(retention_period) = schedule.retention_period.as_ref() {
            let d = std::time::Duration::from(*retention_period);
            let duration = match chrono::Duration::from_std(d) {
                Ok(duration) => duration,
                Err(e) => {
                    tracing::warn!("object {} has invalid duration: {}", doc.name_any(), e);
                    chrono::Duration::max_value()
                }
            };
            vacuum = vacuum.with_retention_period(duration);
        }
    }
    let (result, _) = vacuum.await?;
    metrics::increment_counter!(
        "vacuum_executed_count",
        "table" => doc.name_any(),
        "namespace" => doc.namespace().unwrap()
    );
    Ok(result)
}

/// Updates the schema [`deltalake::DeltaTable`] resource using the `schema` property of
/// the [`DeltaTable`] provided to access it
pub(crate) async fn update_schema(
    doc: &DeltaTable,
    table: DeltaLakeTable,
) -> Result<DeltaLakeTable, Error> {
    match doc.spec.schema_settings.manage.as_ref() {
        Some(true) => {}
        _ => return Ok(table),
    };
    let current_schema = match table.schema() {
        Some(current_schema) => current_schema,
        None => return Ok(table),
    };

    let schema: SchemaTypeStruct = serde_json::de::from_str(&doc.spec.schema_settings.value)?;
    if current_schema == &schema {
        return Ok(table);
    }

    let mut metadata = table.get_metadata()?.clone();
    metadata.schema = schema;
    metadata.created_time = Some(chrono::Utc::now().timestamp_millis());
    let mut writer = deltalake::writer::JsonWriter::for_table(&table)?;
    writer.update_schema(&metadata)?;
    Ok(table)
}
