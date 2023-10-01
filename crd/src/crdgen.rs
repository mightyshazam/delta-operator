use kube::CustomResourceExt;
fn main() {
    print!(
        "{}",
        serde_yaml::to_string(&delta_operator_crd::DeltaTable::crd()).unwrap()
    )
}
