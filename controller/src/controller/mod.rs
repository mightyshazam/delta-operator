use chrono::{DateTime, Utc};
use delta_operator_crd::{maintenance::JobSettings, DeltaTable};
use futures::StreamExt;
use kube::{
    api::ListParams,
    runtime::finalizer::Event as Finalizer,
    runtime::{
        controller::Action,
        events::{Recorder, Reporter},
        finalizer,
        watcher::Config,
        Controller,
    },
    Api, Client, Resource, ResourceExt,
};
use serde::Serialize;
use std::{sync::Arc, time::Duration};
use tokio::sync::RwLock;

use crate::error::Error;

use self::state::State;
pub mod host;
pub mod state;
const DELTA_TABLE_FINALIZER: &str = "deltatables.delta-operator.rs";

#[derive(Clone, Serialize)]
pub(crate) struct Diagnostics {
    #[serde(deserialize_with = "from_ts")]
    pub last_event: DateTime<Utc>,
    #[serde(skip)]
    pub reporter: Reporter,
}

impl Default for Diagnostics {
    fn default() -> Self {
        Self {
            last_event: Utc::now(),
            reporter: "forwardedservice-controller".into(),
        }
    }
}

impl Diagnostics {
    fn recorder(&self, client: Client, doc: &DeltaTable) -> Recorder {
        Recorder::new(client, self.reporter.clone(), doc.object_ref(&()))
    }
}

pub async fn start_controller(state: State, listen_address: String) -> Result<(), Error> {
    let b = Box::new(listen_address);
    let jh = tokio::spawn(host::start_host(Box::leak(b)));
    start(state).await;
    jh.await.unwrap()
}

#[derive(Clone)]
pub struct Context {
    /// Kubernetes client
    pub(crate) client: Client,
    /// Diagnostics read by the web server
    pub(crate) diagnostics: Arc<RwLock<Diagnostics>>,

    /// Job settings for workers
    pub(crate) settings: Arc<JobSettings>,
    // Prometheus metrics
    // pub metrics: Metrics,
}

async fn start(controller_state: State) {
    let client = Client::try_default()
        .await
        .map_err(|e| Error::KubeClient { source: e })
        .expect("failed to create kubernetes client");
    let api = Api::<DeltaTable>::all(client.clone());
    if let Err(e) = api.list(&ListParams::default().limit(1)).await {
        tracing::error!("Installation: cargo run --bin crdgen | kubectl apply -f -");
        panic!("crds are not installed: {}", Error::KubeCrd { source: e });
    }

    let (ctx, cfg) = controller_state.to_context_and_config(client);
    Controller::new(api, cfg.any_semantic())
        .shutdown_on_signal()
        .run(reconcile, error_policy, ctx)
        .filter_map(|x| async move { std::result::Result::ok(x) })
        .for_each(|_| futures::future::ready(()))
        .await;
}

fn error_policy(_: Arc<DeltaTable>, error: &Error, _: Arc<Context>) -> Action {
    tracing::warn!("reconcile failed: {:?}", error);
    // ctx.metrics.reconcile_failure(&doc, error);
    Action::requeue(Duration::from_secs(5 * 60))
}

async fn reconcile(svc: Arc<DeltaTable>, ctx: Arc<Context>) -> Result<Action, Error> {
    ctx.diagnostics.write().await.last_event = Utc::now();
    let ns = svc.namespace().unwrap(); // doc is namespace scoped
    let docs: Api<DeltaTable> = Api::namespaced(ctx.client.clone(), &ns);

    tracing::info!("Reconciling DeltaTable \"{}\" in {}", svc.name_any(), ns);
    finalizer(&docs, DELTA_TABLE_FINALIZER, svc, |event| async {
        let client = ctx.client.clone();
        match event {
            Finalizer::Apply(doc) => {
                let recorder = ctx.diagnostics.read().await.recorder(client.clone(), &doc);
                doc.reconcile(client, recorder, &ctx.settings)
                    .await
                    .map_err(|e| Error::ReconcilationError { source: e })
            }
            Finalizer::Cleanup(doc) => {
                let recorder = ctx.diagnostics.read().await.recorder(client.clone(), &doc);
                doc.cleanup(client, recorder)
                    .await
                    .map_err(|e| Error::ReconcilationError { source: e })
            }
        }
    })
    .await
    .map_err(|e| Error::FinalizerError(Box::new(e)))
}
