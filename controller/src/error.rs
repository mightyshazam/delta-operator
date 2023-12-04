use thiserror::Error;

#[derive(Error, Debug)]
pub enum Error {
    #[error("invalid kubernetes configuration: {source}")]
    KubeConfig {
        #[from]
        source: kube::config::KubeconfigError,
    },
    #[error("unable to create kubernetes client: {source}")]
    KubeClient { source: kube::Error },
    #[error("unable to query kubernetes crd: {source}")]
    KubeCrd { source: kube::Error },
    #[error("server error: {0}")]
    Server(String),
    #[error("reconcilation failed: {source}")]
    ReconcilationError {
        #[from]
        source: delta_operator_crd::Error,
    },
}
