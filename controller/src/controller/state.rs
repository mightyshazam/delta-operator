use delta_operator_crd::maintenance::JobSettings;
use std::sync::Arc;
use tokio::sync::RwLock;

use kube::{runtime::watcher::Config, Client};

use super::{Context, Diagnostics};

pub struct State {
    /// Diagnostics populated by the reconciler
    diagnostics: Arc<RwLock<Diagnostics>>,
    job_settings: Arc<JobSettings>,
    label_selector: Option<String>,
}

impl State {
    pub fn new(settings: JobSettings, label_selector: Option<String>) -> Self {
        Self {
            diagnostics: Arc::new(RwLock::new(Diagnostics::default())),
            job_settings: Arc::new(settings),
            label_selector,
        }
    }

    pub(crate) fn to_context_and_config(&self, client: Client) -> (Arc<Context>, Config) {
        (
            self.to_context(client),
            Config {
                label_selector: self.label_selector.clone(),
                ..Default::default()
            },
        )
    }

    fn to_context(&self, client: Client) -> Arc<Context> {
        Arc::new(Context {
            client,
            // metrics: Metrics::default().register(&self.registry).unwrap(),
            diagnostics: self.diagnostics.clone(),
            settings: self.job_settings.clone(),
        })
    }
}
