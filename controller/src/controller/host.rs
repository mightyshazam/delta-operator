use std::net::SocketAddr;

use axum::{routing::get, Router};
use axum_prometheus::PrometheusMetricLayerBuilder;

use crate::error::Error;

pub(crate) async fn start_host(address: &str) -> Result<(), Error> {
    let app = create_router();
    let addr: SocketAddr = match address.parse() {
        Ok(addr) => addr,
        Err(e) => return Err(Error::Server(e.to_string())),
    };
    axum::Server::bind(&addr)
        .serve(app.into_make_service())
        .await
        .map_err(|e| Error::Server(e.to_string()))
}

fn create_router() -> Router {
    let (prometheus_layer, metric_handle) = PrometheusMetricLayerBuilder::new()
        .with_ignore_patterns(&["/metrics", "/sensitive", "/health"])
        .with_default_metrics()
        .build_pair();

    tracing_subscriber::fmt()
        .with_target(false)
        .compact()
        .init();

    Router::new()
        .route("/metrics", get(|| async move { metric_handle.render() }))
        .route("/health", get(|| async move {}))
        .layer(prometheus_layer)
}
