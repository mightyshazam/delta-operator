use std::collections::BTreeMap;

use app::Arguments;
use clap::Parser;
use delta_operator_crd::maintenance::JobSettings;
use k8s_openapi::{
    api::core::v1::ResourceRequirements, apimachinery::pkg::api::resource::Quantity,
};
mod app;
mod controller;
mod error;
#[tokio::main]
async fn main() {
    let Arguments {
        listen_address,
        image,
        worker_labels,
        worker_annotations,
        worker_service_account,
        worker_max_cpu,
        worker_max_ram,
        worker_namespace,
        label_selector,
        resync_interval,
    } = app::Arguments::parse();
    let settings = JobSettings {
        image,
        labels: vec_to_map(worker_labels),
        annotations: vec_to_map(worker_annotations),
        service_account: worker_service_account,
        resource_requirements: make_resource_requirements(worker_max_cpu, worker_max_ram),
        namespace: worker_namespace,
        resync_interval: Some(resync_interval),
    };

    let state = controller::state::State::new(settings, label_selector);
    controller::start_controller(state, listen_address)
        .await
        .unwrap();
}

fn vec_to_map(args: Vec<(String, String)>) -> BTreeMap<String, String> {
    let mut map = BTreeMap::<String, String>::new();
    for value in args.into_iter() {
        map.insert(value.0, value.1);
    }
    map
}

fn make_resource_requirements(
    worker_max_cpu: Option<String>,
    worker_max_ram: Option<String>,
) -> Option<ResourceRequirements> {
    let mut limits = BTreeMap::<String, Quantity>::new();
    if let Some(cpu) = worker_max_cpu {
        limits.insert("cpu".into(), Quantity(cpu));
    }

    if let Some(ram) = worker_max_ram {
        limits.insert("memory".into(), Quantity(ram));
    }

    match limits.len() {
        0 => None,
        _ => Some(ResourceRequirements {
            limits: Some(limits),
            requests: None,
        }),
    }
}

#[cfg(test)]
mod tests {
    use crate::vec_to_map;

    #[test]
    fn test_vec_to_map_empty() {
        let v = Vec::<(String, String)>::new();
        let m = vec_to_map(v);
        assert_eq!(0, m.len())
    }

    #[test]
    fn test_vec_to_map_duplicates() {
        let a = "a".to_owned();
        let v = vec![(a.clone(), "b".to_owned()), (a.clone(), "f".to_owned())];
        let m = vec_to_map(v);
        assert_eq!(1, m.len());
        let result = m.get(&a).unwrap();
        assert_eq!("f", result)
    }

    #[test]
    fn test_vec_to_map_simple() {
        let v = vec![
            ("a".to_owned(), "b".to_owned()),
            ("e".to_owned(), "f".to_owned()),
        ];
        let m = vec_to_map(v);
        assert_eq!(2, m.len())
    }
}
