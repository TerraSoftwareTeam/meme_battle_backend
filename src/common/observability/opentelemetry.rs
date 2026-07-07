// This module provides utilities to set up OpenTelemetry tracing and logging using the OTLP exporter.
// It configures the tracer provider, logger provider, resource attributes, and integrates with tracing-subscriber.
use opentelemetry::trace::TracerProvider;
use opentelemetry::{global, KeyValue};
use opentelemetry_appender_tracing::layer::OpenTelemetryTracingBridge;
use opentelemetry_otlp::{LogExporter, SpanExporter, WithExportConfig};
use opentelemetry_sdk::{
    logs::SdkLoggerProvider,
    trace::SdkTracerProvider,
    Resource,
};
use opentelemetry_semantic_conventions::resource::{SERVICE_NAME, SERVICE_VERSION};
use std::{error::Error, sync::OnceLock};
use tracing_opentelemetry::OpenTelemetryLayer;
use tracing_subscriber::fmt::format::FmtSpan;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt, EnvFilter};

// get_resource initializes a global Resource containing service name and version.
// Uses OnceLock to ensure the resource is created only once.
fn get_resource() -> Resource {
    static RESOURCE: OnceLock<Resource> = OnceLock::new();
    RESOURCE
        .get_or_init(|| {
            Resource::builder()
                .with_attributes(vec![
                    KeyValue::new(SERVICE_NAME, env!("CARGO_PKG_NAME")),
                    KeyValue::new(SERVICE_VERSION, env!("CARGO_PKG_VERSION")),
                ])
                .build()
        })
        .clone()
}

// Read OTLP endpoint from environment, stripping any path suffix (we pass endpoint per-signal).
fn otlp_base_endpoint() -> String {
    dotenvy::dotenv().ok();
    // Strip /v1/traces or similar paths — new OTel 0.32 adds the path automatically.
    let endpoint = std::env::var("OTEL_EXPORTER_OTLP_ENDPOINT")
        .unwrap_or_else(|_| "http://otelcol:4318".to_string());
    // Strip trailing /v1/... paths added by old configs
    let endpoint = endpoint
        .trim_end_matches("/v1/traces")
        .trim_end_matches("/v1/logs")
        .trim_end_matches('/')
        .to_string();
    endpoint
}

// init_traces sets up the OTLP span exporter and builds the SdkTracerProvider.
pub fn init_traces() -> SdkTracerProvider {
    let endpoint = otlp_base_endpoint();

    let exporter = SpanExporter::builder()
        .with_http()
        .with_endpoint(format!("{}/v1/traces", endpoint))
        .build()
        .expect("Failed to create OTLP span exporter");

    SdkTracerProvider::builder()
        .with_batch_exporter(exporter)
        .with_resource(get_resource())
        .build()
}

// init_logs sets up the OTLP log exporter and builds the SdkLoggerProvider.
pub fn init_logs() -> SdkLoggerProvider {
    let endpoint = otlp_base_endpoint();

    let exporter = LogExporter::builder()
        .with_http()
        .with_endpoint(format!("{}/v1/logs", endpoint))
        .build()
        .expect("Failed to create OTLP log exporter");

    SdkLoggerProvider::builder()
        .with_batch_exporter(exporter)
        .with_resource(get_resource())
        .build()
}

pub struct OtelProviders {
    pub tracer_provider: SdkTracerProvider,
    pub logger_provider: SdkLoggerProvider,
}

// setup_tracing_opentelemetry initializes tracing-subscriber with OpenTelemetry integration.
// Returns both providers so they can be gracefully shut down on exit.
pub fn setup_tracing_opentelemetry() -> OtelProviders {
    dotenvy::dotenv().ok();

    // Initialize OTel providers
    let tracer_provider = init_traces();
    global::set_tracer_provider(tracer_provider.clone());
    let logger_provider = init_logs();

    // Configure log level filter from RUST_LOG env or sane defaults.
    // Sane defaults include silencing hyper/h2/reqwest/rustls to prevent telemetry loop spam.
    let filter = EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| "info,sqlx=info,tower_http=info,opentelemetry=info,hyper=info,h2=info,reqwest=info,rustls=info".parse().unwrap());

    // Select format based on environment variable (defaults to JSON in Docker context)
    let log_format = std::env::var("LOG_FORMAT").unwrap_or_else(|_| "json".to_string());

    if log_format.to_lowercase() == "json" {
        let tracer = tracer_provider.tracer(env!("CARGO_PKG_NAME"));
        let otel_trace_layer = OpenTelemetryLayer::new(tracer);
        let otel_log_layer = OpenTelemetryTracingBridge::new(&logger_provider);

        let json_layer = tracing_subscriber::fmt::layer()
            .json()
            .with_span_list(false)
            .with_current_span(true)
            .with_timer(tracing_subscriber::fmt::time::ChronoLocal::rfc_3339());

        tracing_subscriber::registry()
            .with(filter)
            .with(json_layer)
            .with(otel_trace_layer)
            .with(otel_log_layer)
            .init();
    } else {
        let tracer = tracer_provider.tracer(env!("CARGO_PKG_NAME"));
        let otel_trace_layer = OpenTelemetryLayer::new(tracer);
        let otel_log_layer = OpenTelemetryTracingBridge::new(&logger_provider);

        let fmt_layer = tracing_subscriber::fmt::layer()
            .event_format(
                tracing_subscriber::fmt::format()
                    .with_target(false)
                    .with_level(true)
                    .with_thread_ids(true)
                    .with_thread_names(true)
                    .with_timer(tracing_subscriber::fmt::time::ChronoLocal::rfc_3339()),
            )
            .with_span_events(FmtSpan::NEW | FmtSpan::CLOSE);

        tracing_subscriber::registry()
            .with(filter)
            .with(fmt_layer)
            .with(otel_trace_layer)
            .with(otel_log_layer)
            .init();
    }

    OtelProviders { tracer_provider, logger_provider }
}

// shutdown_opentelemetry gracefully shuts down both providers, flushing all pending data.
pub fn shutdown_opentelemetry(
    providers: OtelProviders,
) -> Result<(), Box<dyn Error + Send + Sync + 'static>> {
    if let Err(e) = providers.tracer_provider.shutdown() {
        return Err(format!("tracer provider shutdown: {}", e).into());
    }
    if let Err(e) = providers.logger_provider.shutdown() {
        return Err(format!("logger provider shutdown: {}", e).into());
    }
    Ok(())
}
