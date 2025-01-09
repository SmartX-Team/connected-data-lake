use std::{env, ffi::OsStr};

#[cfg(feature = "opentelemetry-otlp")]
use opentelemetry_otlp as otlp;
#[cfg(feature = "opentelemetry-otlp")]
use opentelemetry_sdk as sdk;
use tracing::{Subscriber, debug, dispatcher};
use tracing_subscriber::{
    Layer, Registry, layer::SubscriberExt, registry::LookupSpan, util::SubscriberInitExt,
};

fn init_once_opentelemetry(level: &OsStr, export: bool) {
    #[cfg(feature = "opentelemetry-otlp")]
    use sdk::runtime::Tokio as Runtime;

    // Skip init if has been set
    if dispatcher::has_been_set() {
        return;
    }

    // // Set default service name
    // {
    //     const SERVICE_NAME_KEY: &str = "OTEL_SERVICE_NAME";
    //     const SERVICE_NAME_VALUE: &str = env!("CARGO_CRATE_NAME");

    //     if env::var_os(SERVICE_NAME_KEY).is_none() {
    //         env::set_var(SERVICE_NAME_KEY, SERVICE_NAME_VALUE);
    //     }
    // }

    fn init_layer_env_filter<S>() -> impl Layer<S>
    where
        S: Subscriber + for<'span> LookupSpan<'span>,
    {
        ::tracing_subscriber::EnvFilter::from_default_env()
    }

    fn init_layer_stdfmt<S>() -> impl Layer<S>
    where
        S: Subscriber + for<'span> LookupSpan<'span>,
    {
        ::tracing_subscriber::fmt::layer()
    }

    #[cfg(all(feature = "opentelemetry-otlp", feature = "opentelemetry-logs"))]
    fn init_layer_otlp_logger<S>() -> impl Layer<S>
    where
        S: Subscriber + for<'span> LookupSpan<'span>,
    {
        let exporter = otlp::LogExporter::builder()
            .with_tonic()
            .build()
            .expect("failed to init a log exporter");

        let provider = sdk::logs::LoggerProvider::builder()
            .with_batch_exporter(exporter, Runtime)
            .build();

        ::opentelemetry_appender_tracing::layer::OpenTelemetryTracingBridge::new(&provider)
    }

    #[cfg(all(feature = "opentelemetry-otlp", feature = "opentelemetry-metrics"))]
    fn init_layer_otlp_metrics<S>() -> impl Layer<S>
    where
        S: Subscriber + for<'span> LookupSpan<'span>,
    {
        let exporter = opentelemetry_otlp::MetricExporter::builder()
            .with_tonic()
            .build()
            .expect("failed to init a metric exporter");

        let reader = sdk::metrics::PeriodicReader::builder(exporter, Runtime).build();

        let meter_provider = sdk::metrics::MeterProviderBuilder::default()
            .with_reader(reader)
            .build();

        ::tracing_opentelemetry::MetricsLayer::new(meter_provider)
    }

    #[cfg(all(feature = "opentelemetry-otlp", feature = "opentelemetry-trace"))]
    fn init_layer_otlp_tracer<S>() -> impl Layer<S>
    where
        S: Subscriber + for<'span> LookupSpan<'span>,
    {
        use opentelemetry::trace::TracerProvider;

        let name = env::var("OTEL_SERVICE_NAME").unwrap_or_else(|_| "cdl-k8s-core".into());

        let exporter = opentelemetry_otlp::SpanExporter::builder()
            .with_tonic()
            .build()
            .expect("failed to init a span exporter");

        let provider = sdk::trace::TracerProvider::builder()
            .with_batch_exporter(exporter, Runtime)
            .build();

        ::tracing_opentelemetry::OpenTelemetryLayer::new(provider.tracer(name))
    }

    let layer = Registry::default()
        .with(init_layer_env_filter())
        .with(init_layer_stdfmt());

    let is_otel_exporter_activated = env::var("OTEL_EXPORTER_OTLP_ENDPOINT").is_ok();
    if export && is_otel_exporter_activated {
        #[cfg(all(feature = "opentelemetry-otlp", feature = "opentelemetry-logs"))]
        let layer = layer.with(init_layer_otlp_logger());
        #[cfg(all(feature = "opentelemetry-otlp", feature = "opentelemetry-metrics"))]
        let layer = layer.with(init_layer_otlp_metrics());
        #[cfg(all(feature = "opentelemetry-otlp", feature = "opentelemetry-trace"))]
        let layer = layer.with(init_layer_otlp_tracer());

        layer.init()
    } else {
        if export && !is_otel_exporter_activated {
            debug!("OTEL exporter is not activated.");
        }

        layer.init()
    }
}

#[inline]
pub fn init_once() {
    init_once_with_default(true)
}

#[inline]
pub fn init_once_with(level: impl AsRef<OsStr>, export: bool) {
    init_once_opentelemetry(level.as_ref(), export)
}

#[inline]
pub fn init_once_with_default(export: bool) {
    let level = env::var_os("RUST_LOG").unwrap_or_else(|| "INFO".into());
    init_once_opentelemetry(&level, export)
}

#[inline]
pub fn init_once_with_level_int(level: u8, export: bool) {
    // You can see how many times a particular flag or argument occurred
    // Note, only flags can have multiple occurrences
    let debug_level = match level {
        0 => "WARN",
        1 => "INFO",
        2 => "DEBUG",
        3 => "TRACE",
        level => panic!("too high debug level: {level}"),
    };
    init_once_opentelemetry(debug_level.as_ref(), export)
}
