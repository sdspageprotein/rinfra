use rinfra_core::config::TelemetryConfig;
use rinfra_core::error::AppError;

#[cfg(feature = "telemetry")]
mod otel;

#[cfg(feature = "telemetry")]
pub use otel::{inject_trace_context, extract_trace_context_span};

/// Guard that flushes and shuts down the OTel tracer provider on drop.
pub struct OtelGuard {
    #[cfg(feature = "telemetry")]
    provider: opentelemetry_sdk::trace::TracerProvider,
}

impl OtelGuard {
    pub fn shutdown(self) {
        #[cfg(feature = "telemetry")]
        {
            if let Err(e) = self.provider.shutdown() {
                tracing::warn!(error = %e, "otel tracer provider shutdown error");
            }
        }
    }
}

/// Result of telemetry initialization: guard + optional OTel layer for tracing subscriber.
pub struct TelemetryInit {
    pub guard: Option<OtelGuard>,
    #[cfg(feature = "telemetry")]
    pub otel_layer: Option<tracing_opentelemetry::OpenTelemetryLayer<
        tracing_subscriber::Registry,
        opentelemetry_sdk::trace::Tracer,
    >>,
}

/// Initialize OpenTelemetry tracing.
///
/// Returns `TelemetryInit` containing the guard and optional OTel layer.
pub fn init_telemetry(
    config: &TelemetryConfig,
    service_name: &str,
) -> Result<TelemetryInit, AppError> {
    if !config.enabled {
        return Ok(TelemetryInit {
            guard: None,
            #[cfg(feature = "telemetry")]
            otel_layer: None,
        });
    }

    #[cfg(feature = "telemetry")]
    {
        let (guard, layer) = otel::init_otel_tracer(config, service_name)?;
        Ok(TelemetryInit {
            guard: Some(guard),
            otel_layer: Some(layer),
        })
    }

    #[cfg(not(feature = "telemetry"))]
    {
        let _ = service_name;
        tracing::warn!("telemetry is enabled in config but the 'telemetry' feature is not compiled in; ignoring");
        Ok(TelemetryInit { guard: None })
    }
}

#[cfg(not(feature = "telemetry"))]
pub fn inject_trace_context() -> Option<std::collections::HashMap<String, String>> {
    None
}

#[cfg(not(feature = "telemetry"))]
pub fn extract_trace_context_span(
    _ctx: &Option<std::collections::HashMap<String, String>>,
    span_name: &str,
) -> tracing::Span {
    tracing::info_span!("cluster.op", name = %span_name)
}
