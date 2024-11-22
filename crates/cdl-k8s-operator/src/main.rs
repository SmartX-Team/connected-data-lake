#![recursion_limit = "256"]

mod ctx;
mod optimizer;
mod validator;

use cdl_k8s_core::k8s_operator::Ctx;
use tokio::join;

pub(crate) mod consts {
    use cdl_k8s_core::env::infer_string;

    pub const NAME: &str = "cdl-operator";

    const ENV_PROMETHEUS_URL: &str = "PROMETHEUS_URL";

    pub fn infer_prometheus_url() -> String {
        infer_string(ENV_PROMETHEUS_URL).unwrap_or_else(|_| {
            "http://kube-prometheus-stack-prometheus.monitoring.svc:9090".into()
        })
    }
}

#[tokio::main]
async fn main() {
    join!(
        self::ctx::model::Ctx::spawn_crd(),
        self::ctx::model_claim::Ctx::spawn_crd(),
        self::ctx::model_storage::Ctx::spawn_crd(),
        self::ctx::model_storage_binding::Ctx::spawn_crd(),
    );
}
