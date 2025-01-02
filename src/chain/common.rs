use malachite_metrics::{Metrics, Registry, SharedRegistry};

pub fn new_metrics() -> Metrics {
    let registry = SharedRegistry::new(Registry::default(), None);
    Metrics::register(&registry)
}
