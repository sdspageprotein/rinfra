use std::collections::HashMap;

use opentelemetry::global;
use opentelemetry::propagation::{Extractor, Injector};
use opentelemetry::trace::TracerProvider as _;
use opentelemetry::KeyValue;
use opentelemetry_sdk::propagation::TraceContextPropagator;
use opentelemetry_sdk::trace::{TracerProvider, Sampler};
use opentelemetry_otlp::WithExportConfig;
use rinfra_core::config::TelemetryConfig;
use rinfra_core::error::{AppError, ErrorCode};
use tracing::info;
use tracing_opentelemetry::OpenTelemetrySpanExt;

use super::OtelGuard;

type OtelLayer = tracing_opentelemetry::OpenTelemetryLayer<
    tracing_subscriber::Registry,
    opentelemetry_sdk::trace::Tracer,
>;

pub fn init_otel_tracer(
    config: &TelemetryConfig,
    service_name: &str,
) -> Result<(OtelGuard, OtelLayer), AppError> {
    global::set_text_map_propagator(TraceContextPropagator::new());

    let exporter = opentelemetry_otlp::SpanExporter::builder()
        .with_tonic()
        .with_endpoint(&config.otlp_endpoint)
        .build()
        .map_err(|e| AppError::new(ErrorCode::Internal, format!("otlp exporter init failed: {e}")))?;

    let sampler = if (config.sample_ratio - 1.0).abs() < f64::EPSILON {
        Sampler::AlwaysOn
    } else if config.sample_ratio <= 0.0 {
        Sampler::AlwaysOff
    } else {
        Sampler::TraceIdRatioBased(config.sample_ratio)
    };

    let resource = opentelemetry_sdk::Resource::new(vec![
        KeyValue::new("service.name", service_name.to_string()),
    ]);

    let provider = TracerProvider::builder()
        .with_batch_exporter(exporter, opentelemetry_sdk::runtime::Tokio)
        .with_sampler(sampler)
        .with_resource(resource)
        .build();

    global::set_tracer_provider(provider.clone());

    let tracer = provider.tracer(service_name.to_string());
    let layer = tracing_opentelemetry::layer().with_tracer(tracer);

    info!(
        endpoint = %config.otlp_endpoint,
        sample_ratio = config.sample_ratio,
        service = %service_name,
        "opentelemetry tracer initialized"
    );

    Ok((OtelGuard { provider }, layer))
}

struct HashMapInjector<'a>(&'a mut HashMap<String, String>);

impl Injector for HashMapInjector<'_> {
    fn set(&mut self, key: &str, value: String) {
        self.0.insert(key.to_string(), value);
    }
}

struct HashMapExtractor<'a>(&'a HashMap<String, String>);

impl Extractor for HashMapExtractor<'_> {
    fn get(&self, key: &str) -> Option<&str> {
        self.0.get(key).map(|v| v.as_str())
    }

    fn keys(&self) -> Vec<&str> {
        self.0.keys().map(|k| k.as_str()).collect()
    }
}

/// Inject current span's trace context into a HashMap for cross-process propagation.
pub fn inject_trace_context() -> Option<HashMap<String, String>> {
    let mut map = HashMap::new();
    global::get_text_map_propagator(|p| {
        let cx = tracing::Span::current().context();
        p.inject_context(&cx, &mut HashMapInjector(&mut map));
    });
    if map.is_empty() {
        None
    } else {
        Some(map)
    }
}

/// Extract trace context from a HashMap and create a child span.
pub fn extract_trace_context_span(
    trace_ctx: &Option<HashMap<String, String>>,
    span_name: &str,
) -> tracing::Span {
    match trace_ctx {
        Some(ctx) if !ctx.is_empty() => {
            let parent_cx = global::get_text_map_propagator(|p| {
                p.extract(&HashMapExtractor(ctx))
            });
            let span = tracing::info_span!("cluster.op", name = %span_name);
            span.set_parent(parent_cx);
            span
        }
        _ => {
            tracing::info_span!("cluster.op", name = %span_name)
        }
    }
}
