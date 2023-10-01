use app::Arguments;
use clap::Parser;
use delta_operator_crd::maintenance::{
    checkpoint_table, optimize_table, vacuum_table, Action, ENV_WORKER_POD_NAME,
};
use delta_operator_crd::DeltaTable;
use kube::api::{Patch, PatchParams};
use kube::runtime::events::{Recorder, Reporter};
use kube::{Api, Client, Resource};
use serde_json::json;
mod app;

#[tokio::main]
async fn main() {
    let pod_name =
        std::env::var(ENV_WORKER_POD_NAME).unwrap_or(app::DEFAULT_WORKER_NAME.to_owned());
    let args = app::Arguments::parse();
    let client = match Client::try_default().await {
        Ok(client) => client,
        Err(err) => panic!("unable to initialize kubernetes client: {}", err),
    };
    let api: Api<DeltaTable> = Api::namespaced(client.clone(), &args.namespace);
    let table = match api.get(&args.table).await {
        Ok(table) => table,
        Err(err) => panic!(
            "unable to retrieve delta table resource `{}`: {}",
            args.table, err
        ),
    };

    let delta_lake_table = match table.delta_lake_table(client.clone()).await {
        Ok(dt) => dt,
        Err(dt) => panic!("unable to access delta table `{}`: {}", args.table, dt),
    };
    let result = match args.action {
        Action::Checkpoint => checkpoint_table(&table, delta_lake_table).await,
        Action::Optimize => optimize_table(&table, delta_lake_table).await,
        Action::Vacuum => vacuum_table(&table, delta_lake_table).await,
    };

    let reporter = Reporter {
        controller: args.worker_name.as_ref().unwrap().to_owned(),
        instance: Some(pod_name),
    };

    let recorder = Recorder::new(client, reporter, table.object_ref(&()));
    match result {
        Ok(dt) => match args.action {
            Action::Checkpoint => {
                publish_success(
                    &api,
                    &recorder,
                    &args,
                    json!({
                        "status": {
                            "last_checkpoint_commit": dt.version(),
                            "last_checkpoint_timestamp": chrono::Utc::now().timestamp(),
                        }
                    }),
                )
                .await
            }
            Action::Optimize => {
                publish_success(
                    &api,
                    &recorder,
                    &args,
                    json!({
                        "status": {
                            "last_optimize_commit": dt.version(),
                            "last_optimize_timestamp": chrono::Utc::now().timestamp(),
                        }
                    }),
                )
                .await
            }
            Action::Vacuum => {
                publish_success(
                    &api,
                    &recorder,
                    &args,
                    json!({
                        "status": {
                            "last_vacuum_commit": dt.version(),
                            "last_vacuum_timestamp": chrono::Utc::now().timestamp(),
                        }
                    }),
                )
                .await
            }
        },
        Err(err) => {
            panic!(
                "failed to perform maintence {} on delta table `{}`: {}",
                args.action, args.table, err
            );
        }
    };
}

async fn publish_success(
    api: &Api<DeltaTable>,
    recorder: &Recorder,
    args: &Arguments,
    value: serde_json::Value,
) {
    let pp = PatchParams::default();
    if let Err(e) = api
        .patch_status(&args.table, &pp, &Patch::Merge(value))
        .await
    {
        panic!("failed to patch delta table: {}", e);
    }
    if let Err(e) = recorder
        .publish(kube::runtime::events::Event {
            type_: kube::runtime::events::EventType::Normal,
            reason: format!("{} requested by controller", args.action),
            note: Some("Success".into()),
            action: format!("{}", args.action),
            secondary: None,
        })
        .await
    {
        tracing::error!("failed to record delta table changes: {}", e);
    }
}
