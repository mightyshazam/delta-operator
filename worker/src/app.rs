use clap::Parser;
use delta_operator_crd::maintenance::Action;

pub const DEFAULT_WORKER_NAME: &str = "delta-operator-worker";

#[derive(Parser, Debug)]
#[clap(author = "Author Name", version, about)]
/// A controller for port forwarding
pub struct Arguments {
    #[clap(long, required = true)]
    pub table: String,

    #[clap(long, required = true)]
    pub namespace: String,

    #[arg(value_enum)]
    #[clap(long)]
    pub action: Action,
    #[clap(long, env, default_value = DEFAULT_WORKER_NAME)]
    pub worker_name: Option<String>,
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use super::Arguments;
    use clap::Parser;
    use delta_operator_crd::maintenance::Action;

    #[test]
    fn test_all_arguments() {
        let arguments = Arguments::parse_from(vec![
            "worker",
            "--table",
            "test-table",
            "--namespace",
            "something",
            "--action",
            "optimize",
            "--worker-name",
            "test-worker",
        ]);
        assert_eq!("test-table", arguments.table);
        assert!(matches!(arguments.action, Action::Optimize));
        assert_eq!("test-worker", arguments.worker_name.unwrap());
        assert_eq!("something", arguments.namespace);
    }

    #[test]
    fn test_all_actions() {
        let mut values: HashMap<&str, Action> = HashMap::with_capacity(3);
        values.insert("checkpoint", Action::Checkpoint);
        values.insert("optimize", Action::Optimize);
        values.insert("vacuum", Action::Vacuum);
        for (k, v) in values {
            let arguments = Arguments::parse_from(vec![
                "worker",
                "--table",
                "test-table",
                "--action",
                k,
                "--worker-name",
                "test-worker",
                "--namespace",
                "something",
            ]);

            assert_eq!("test-table", arguments.table);
            assert!(arguments.action == v);
            assert_eq!("test-worker", arguments.worker_name.unwrap());
            assert_eq!("something", arguments.namespace);
        }
    }
}
