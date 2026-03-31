use rinfra_core::config::{FileLogConfig, LogPluginConfigs, StdoutLogConfig};
use rinfra_core::error::{AppError, ErrorCode};
use tracing::info;
use tracing_appender::rolling;
use tracing_subscriber::fmt;
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::util::SubscriberInitExt;
use tracing_subscriber::{EnvFilter, Layer, Registry};

/// Initialize the tracing subscriber based on config.
///
/// Supports stdout-only, file-only, or dual-layer (stdout + file) output.
/// When `otel_layer` is provided, it is added to the subscriber pipeline.
pub fn init_observability(
    config: &LogPluginConfigs,
    #[cfg(feature = "telemetry")]
    otel_layer: Option<tracing_opentelemetry::OpenTelemetryLayer<Registry, opentelemetry_sdk::trace::Tracer>>,
) -> Result<(), AppError> {
    let stdout_enabled = config.stdout.enabled;
    let file_enabled = config.file.enabled;

    #[cfg(feature = "telemetry")]
    let has_otel = otel_layer.is_some();
    #[cfg(not(feature = "telemetry"))]
    let has_otel = false;

    if !stdout_enabled && !file_enabled && !has_otel {
        return Ok(());
    }

    let result = init_combined(
        config,
        #[cfg(feature = "telemetry")]
        otel_layer,
    );

    if let Err(e) = result {
        let msg = e.to_string();
        if !msg.contains("already been set") {
            return Err(AppError::new(
                ErrorCode::LogInitFailed,
                format!("failed to initialize logging: {e}"),
            ));
        }
    }

    info!(
        stdout = stdout_enabled,
        file = file_enabled,
        otel = has_otel,
        "observability initialized"
    );

    Ok(())
}

/// Backward-compatible wrapper that calls init_observability without OTel.
pub fn init_logging(config: &LogPluginConfigs) -> Result<(), AppError> {
    init_observability(
        config,
        #[cfg(feature = "telemetry")]
        None,
    )
}

fn init_combined(
    config: &LogPluginConfigs,
    #[cfg(feature = "telemetry")]
    otel_layer: Option<tracing_opentelemetry::OpenTelemetryLayer<Registry, opentelemetry_sdk::trace::Tracer>>,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let mut layers: Vec<Box<dyn Layer<Registry> + Send + Sync>> = Vec::new();

    if config.stdout.enabled {
        layers.push(build_stdout_layer(&config.stdout)?);
    }
    if config.file.enabled {
        layers.push(build_file_layer(&config.file)?);
    }

    #[cfg(feature = "telemetry")]
    if let Some(otel) = otel_layer {
        layers.push(Box::new(otel));
    }

    if layers.is_empty() {
        return Ok(());
    }

    tracing_subscriber::registry()
        .with(layers)
        .try_init()?;
    Ok(())
}

fn build_stdout_layer(
    config: &StdoutLogConfig,
) -> Result<Box<dyn Layer<Registry> + Send + Sync>, Box<dyn std::error::Error + Send + Sync>> {
    let filter = make_filter(&config.level)?;
    Ok(match config.format.as_str() {
        "json" => fmt::layer()
            .json()
            .flatten_event(true)
            .with_filter(filter)
            .boxed(),
        _ => fmt::layer()
            .with_filter(filter)
            .boxed(),
    })
}

fn build_file_layer(
    config: &FileLogConfig,
) -> Result<Box<dyn Layer<Registry> + Send + Sync>, Box<dyn std::error::Error + Send + Sync>> {
    let filter = make_filter(&config.level)?;
    let appender = make_rolling_appender(config);
    Ok(match config.format.as_str() {
        "json" => fmt::layer()
            .with_writer(appender)
            .with_ansi(false)
            .json()
            .flatten_event(true)
            .with_filter(filter)
            .boxed(),
        _ => fmt::layer()
            .with_writer(appender)
            .with_ansi(false)
            .with_filter(filter)
            .boxed(),
    })
}

fn make_filter(level: &str) -> Result<EnvFilter, Box<dyn std::error::Error + Send + Sync>> {
    EnvFilter::try_new(level).map_err(|e| {
        Box::new(AppError::new(
            ErrorCode::LogInitFailed,
            format!("invalid log level '{level}': {e}"),
        )) as Box<dyn std::error::Error + Send + Sync>
    })
}

fn make_rolling_appender(config: &FileLogConfig) -> rolling::RollingFileAppender {
    let rotation = match config.rotation.as_str() {
        "hourly" => rolling::Rotation::HOURLY,
        "never" => rolling::Rotation::NEVER,
        _ => rolling::Rotation::DAILY,
    };

    rolling::Builder::new()
        .rotation(rotation)
        .filename_prefix(&config.filename)
        .max_log_files(config.max_files as usize)
        .build(&config.path)
        .expect("failed to create rolling file appender")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_init_logging_both_disabled_returns_ok() {
        let config = LogPluginConfigs {
            stdout: StdoutLogConfig {
                enabled: false,
                level: "info".to_string(),
                format: "pretty".to_string(),
            },
            file: FileLogConfig {
                enabled: false,
                ..FileLogConfig::default()
            },
        };
        assert!(init_logging(&config).is_ok());
    }

    #[test]
    fn test_make_filter_valid() {
        let filter = make_filter("info");
        assert!(filter.is_ok());
    }

    #[test]
    fn test_make_filter_complex() {
        let filter = make_filter("info,rinfra_plugins=debug");
        assert!(filter.is_ok());
    }

    #[test]
    fn test_make_rolling_appender_daily() {
        let config = FileLogConfig::default();
        let _appender = make_rolling_appender(&config);
    }

    #[test]
    fn test_make_rolling_appender_hourly() {
        let config = FileLogConfig {
            rotation: "hourly".to_string(),
            ..FileLogConfig::default()
        };
        let _appender = make_rolling_appender(&config);
    }

    #[test]
    fn test_make_rolling_appender_never() {
        let config = FileLogConfig {
            rotation: "never".to_string(),
            ..FileLogConfig::default()
        };
        let _appender = make_rolling_appender(&config);
    }

    #[test]
    fn test_file_log_config_defaults() {
        let config = FileLogConfig::default();
        assert!(!config.enabled);
        assert_eq!(config.path, "logs");
        assert_eq!(config.filename, "app.log");
        assert_eq!(config.rotation, "daily");
        assert_eq!(config.max_files, 7);
        assert_eq!(config.level, "info");
        assert_eq!(config.format, "json");
    }
}
