use clap::Parser;
use std::time::Duration;
use std::{error::Error, str::FromStr};
const DEFAULT_LISTEN_ADDRES: &str = "0.0.0.0:8080";

#[derive(Parser, Debug)]
#[clap(author = "Author Name", version, about)]
/// A controller for port forwarding
pub(crate) struct Arguments {
    #[clap(long, env, required = false, default_value = DEFAULT_LISTEN_ADDRES)]
    pub listen_address: String,
    #[clap(long, env, required = true)]
    pub image: String,
    #[clap(long, env, value_parser = parse_key_val::<String, String>)]
    pub worker_labels: Vec<(String, String)>,
    #[clap(long, env, value_parser = parse_key_val::<String, String>)]
    pub worker_annotations: Vec<(String, String)>,
    #[clap(long, env, default_value = "delta-operator-system")]
    pub worker_namespace: String,
    #[clap(long, env, required = true)]
    pub worker_service_account: String,
    #[clap(long, env)]
    pub worker_max_cpu: Option<String>,
    #[clap(long, env)]
    pub worker_max_ram: Option<String>,
    #[clap(long, env)]
    pub label_selector: Option<String>,
    #[clap(long, env, value_parser = parse_duration, default_value = "1h")]
    pub resync_interval: Duration,
}

/// Parse a single key-value pair
fn parse_key_val<T, U>(s: &str) -> Result<(T, U), Box<dyn Error + Send + Sync + 'static>>
where
    T: std::str::FromStr,
    T::Err: Error + Send + Sync + 'static,
    U: std::str::FromStr,
    U::Err: Error + Send + Sync + 'static,
{
    let pos = s
        .find('=')
        .ok_or_else(|| format!("invalid KEY=value: no `=` found in `{s}`"))?;
    Ok((s[..pos].parse()?, s[pos + 1..].parse()?))
}

fn parse_duration(s: &str) -> Result<Duration, Box<dyn Error + Send + Sync + 'static>> {
    match kube::core::Duration::from_str(s) {
        Ok(d) => match d.is_negative() {
            true => Err("negative durations are not allowed".into()),
            false => Ok(Duration::from(d)),
        },
        Err(e) => Err(Box::new(e)),
    }
}

#[cfg(test)]
mod tests {
    use crate::app::DEFAULT_LISTEN_ADDRES;

    use super::Arguments;
    use clap::Parser;

    #[test]
    fn test_all_arguments() {
        let arguments = Arguments::parse_from(vec![
            "app",
            "--listen-address",
            "0.0.0.0:443",
            "--worker-namespace",
            "default",
            "--image",
            "test-image:vtest",
            "--worker-labels",
            "this=true",
            "--worker-annotations",
            "annotated=my annotations",
            "--worker-service-account",
            "test-service-account",
            "--worker-max-cpu",
            "500m",
            "--worker-max-ram",
            "1Gi",
            "--resync-interval",
            "5m",
        ]);
        assert_eq!("0.0.0.0:443", arguments.listen_address);
        assert_eq!("test-image:vtest", arguments.image);
        assert!(arguments.worker_labels.len() == 1);
        assert!(arguments.worker_annotations.len() == 1);
        assert_eq!("test-service-account", arguments.worker_service_account);
        assert_eq!("500m", arguments.worker_max_cpu.unwrap());
        assert_eq!("1Gi", arguments.worker_max_ram.unwrap());
        assert_eq!("default", arguments.worker_namespace);
        assert_eq!(300, arguments.resync_interval.as_secs());
    }

    #[test]
    fn test_default_arguments() {
        let arguments = Arguments::parse_from(vec![
            "app",
            "--image",
            "test-image:vtest",
            "--worker-service-account",
            "test-service-account",
        ]);
        assert_eq!(DEFAULT_LISTEN_ADDRES, arguments.listen_address);
        assert_eq!("test-image:vtest", arguments.image);
        assert!(arguments.worker_labels.is_empty());
        assert!(arguments.worker_annotations.is_empty());
        assert_eq!("test-service-account", arguments.worker_service_account);
        assert!(arguments.worker_max_cpu.is_none());
        assert!(arguments.worker_max_ram.is_none());
        assert_eq!("delta-operator-system", arguments.worker_namespace);
        assert_eq!(3600, arguments.resync_interval.as_secs());
    }
}
