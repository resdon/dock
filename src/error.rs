/// Application-level error handling for Dock.
///
/// # Purpose
/// Provides application-level error propagation using `anyhow` with context,
/// and `color-eyre` for development-time diagnostics with backtraces.
/// This module bridges the strongly-typed wgpu-api errors with application-level
/// error handling patterns.
///
/// # Design
/// - `anyhow::Result`: Application-level functions return `AppResult<T>`.
/// - `color-eyre`: Development-only error reporter with backtraces and context.
/// - `tracing`: All errors are logged before propagation.
/// - `miette`: Human-readable diagnostics for critical errors.
///
/// # Ownership
/// Application errors are created in `src/` and may wrap `wgpu_api::DockError`
/// or other subsystem errors with additional context.
///
/// # Dependencies
/// Depends on `wgpu_api::error` for subsystem-specific error types.
use thiserror::Error;

// ---------------------------------------------------------------------------
// Application-level error type
// ---------------------------------------------------------------------------

/// Top-level application error type.
///
/// Wraps errors from all subsystems with application-specific context.
/// Use `AppResult<T>` as the return type for application-level functions.
///
/// # Examples
/// ```
/// use dock::error::AppResult;
///
/// async fn initialize_dock() -> AppResult<()> {
///     // ... initialization code
///     Ok(())
/// }
/// ```
#[derive(Debug, Error)]
pub enum AppError {
    #[error("wgpu-api error: {0}")]
    WgpuApi(#[from] wgpu_api::DockError),

    #[error("Application initialization failed: {0}")]
    InitFailed(String),

    #[error("Configuration error: {0}")]
    ConfigurationError(String),

    #[error("Service error: {0}")]
    ServiceError(String),

    #[error("Listener error: {0}")]
    ListenerError(String),

    #[error("State management error: {0}")]
    StateError(String),

    #[error("Event handling error: {0}")]
    EventError(String),

    #[error("Resolver error: {0}")]
    ResolverError(String),

    #[error("D-Bus error: {0}")]
    DbusError(String),

    #[error("System monitoring error: {0}")]
    SystemMonitorError(String),

    #[error("Icon loading error: {0}")]
    IconError(String),

    #[error("Process resolution error: {0}")]
    ProcessError(String),

    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
}

// ---------------------------------------------------------------------------
// Result type alias
// ---------------------------------------------------------------------------

/// Result type alias for application-level operations.
///
/// # Examples
/// ```
/// use dock::error::AppResult;
///
/// fn load_config() -> AppResult<String> {
///     Ok("config".to_string())
/// }
/// ```
pub type AppResult<T> = Result<T, AppError>;

// ---------------------------------------------------------------------------
// Miette integration
// ---------------------------------------------------------------------------

impl miette::Diagnostic for AppError {
    fn code<'a>(&'a self) -> Option<Box<dyn std::fmt::Display + 'a>> {
        Some(Box::new(match self {
            AppError::WgpuApi(_) => "WGPU_API_ERROR",
            AppError::InitFailed(_) => "INIT_FAILED",
            AppError::ConfigurationError(_) => "CONFIG_ERROR",
            AppError::ServiceError(_) => "SERVICE_ERROR",
            AppError::ListenerError(_) => "LISTENER_ERROR",
            AppError::StateError(_) => "STATE_ERROR",
            AppError::EventError(_) => "EVENT_ERROR",
            AppError::ResolverError(_) => "RESOLVER_ERROR",
            AppError::DbusError(_) => "DBUS_ERROR",
            AppError::SystemMonitorError(_) => "MONITOR_ERROR",
            AppError::IconError(_) => "ICON_ERROR",
            AppError::ProcessError(_) => "PROCESS_ERROR",
            AppError::Io(_) => "IO_ERROR",
        }))
    }

    fn severity(&self) -> Option<miette::Severity> {
        Some(match self {
            AppError::InitFailed(_)
            | AppError::ConfigurationError(_)
            | AppError::WgpuApi(wgpu_api::DockError::Gpu(_)) => miette::Severity::Error,
            _ => miette::Severity::Warning,
        })
    }

    fn help<'a>(&'a self) -> Option<Box<dyn std::fmt::Display + 'a>> {
        Some(Box::new(match self {
            AppError::InitFailed(msg) => {
                format!(
                    "Initialization failed: {}. Check system requirements and permissions.",
                    msg
                )
            }
            AppError::ConfigurationError(msg) => {
                format!(
                    "Configuration error: {}. Check your configuration files.",
                    msg
                )
            }
            AppError::DbusError(msg) => {
                format!("D-Bus error: {}. Ensure D-Bus session is running.", msg)
            }
            _ => return None,
        }))
    }
}

// ---------------------------------------------------------------------------
// Color-eyre integration (development only)
// ---------------------------------------------------------------------------

static ERROR_REPORTER_INIT: std::sync::OnceLock<Result<(), String>> = std::sync::OnceLock::new();

/// Initialize the color-eyre error reporter.
///
/// Idempotent: repeated calls after a successful install are treated as success.
/// If a previous install attempt failed, the original failure reason is returned.
///
/// # Examples
/// ```no_run
/// use dock::error::init_error_reporter;
///
/// #[tokio::main]
/// async fn main() -> anyhow::Result<()> {
///     init_error_reporter()?;
///     // ... rest of application
///     Ok(())
/// }
/// ```
pub fn init_error_reporter() -> anyhow::Result<()> {
    match ERROR_REPORTER_INIT.get_or_init(|| color_eyre::install().map_err(|e| e.to_string())) {
        Ok(()) => Ok(()),
        Err(e) => Err(anyhow::anyhow!(
            "Failed to install color-eyre error reporter: {}",
            e
        )),
    }
}

// ---------------------------------------------------------------------------
// Tracing integration
// ---------------------------------------------------------------------------

/// Initialize the tracing subscriber with error-level filtering.
///
/// # Examples
/// ```no_run
/// use dock::error::init_tracing;
///
/// fn main() {
///     init_tracing("dock=debug,wgpu_api=info");
/// }
/// ```
///
/// # Arguments
/// * `default_filter` - Default log filter (e.g., `"dock=debug,wgpu_api=info"`).
///   Can be overridden by `RUST_LOG` environment variable.
pub fn init_tracing(default_filter: &str) {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new(default_filter)),
        )
        .with_target(true)
        .with_thread_ids(true)
        .with_line_number(true)
        .init();
}

/// Initialize both tracing and color-eyre for development.
///
/// # Examples
/// ```no_run
/// use dock::error::init_development;
///
/// #[tokio::main]
/// async fn main() -> anyhow::Result<()> {
///     init_development()?;
///     // ... rest of application
///     Ok(())
/// }
/// ```
pub fn init_development() -> anyhow::Result<()> {
    init_tracing("dock=debug,wgpu_api=info,tracing=warn");
    init_error_reporter()
}

// ---------------------------------------------------------------------------
// Context helpers
// ---------------------------------------------------------------------------

/// Add context to a result, logging the error if it fails.
///
/// Combines `anyhow::Context` with `tracing::error!` for automatic
/// error logging before propagation.
///
/// # Examples
/// ```
/// use dock::error::log_and_context;
///
/// async fn load_data() -> anyhow::Result<Vec<u8>> {
///     let data = std::fs::read("data.bin")
///         .map_err(|e| log_and_context(e, "Failed to read data file"))?;
///     Ok(data)
/// }
/// ```
pub fn log_and_context<T, E: std::error::Error + std::fmt::Debug + Send + Sync + 'static>(
    result: Result<T, E>,
    context: impl Into<String>,
) -> Result<T, anyhow::Error> {
    let context = context.into();
    result.map_err(|e| {
        tracing::error!(error = %e, error.debug = ?e, context);
        anyhow::Error::new(e).context(context)
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_app_error_from_wgpu_api() {
        let wgpu_err = wgpu_api::DockError::Gpu(wgpu_api::GpuError::OutOfMemory);
        let app_err: AppError = wgpu_err.into();
        assert!(app_err.to_string().contains("wgpu-api error"));
    }

    #[test]
    fn test_app_error_to_anyhow() {
        let app_err = AppError::InitFailed("test".to_string());
        let anyhow_err: anyhow::Error = app_err.into();
        assert!(anyhow_err
            .to_string()
            .contains("Application initialization failed"));
    }

    #[test]
    fn test_app_result_ok() {
        let result: AppResult<i32> = Ok(42);
        assert_eq!(result.unwrap(), 42);
    }

    #[test]
    fn test_app_result_err() {
        let result: AppResult<i32> = Err(AppError::ConfigurationError("bad config".into()));
        assert!(result.is_err());
    }

    #[test]
    fn test_miette_code_init_failed() {
        let err = AppError::InitFailed("test".into());
        let code = miette::Diagnostic::code(&err);
        assert!(code.is_some());
        assert!(format!("{}", code.unwrap()).contains("INIT_FAILED"));
    }

    #[test]
    fn test_miette_severity_init_failed() {
        let err = AppError::InitFailed("test".into());
        assert!(matches!(
            miette::Diagnostic::severity(&err),
            Some(miette::Severity::Error)
        ));
    }

    #[test]
    fn test_miette_help_dbus() {
        let err = AppError::DbusError("connection refused".into());
        let help = miette::Diagnostic::help(&err);
        assert!(help.is_some());
        let help_str = format!("{}", help.unwrap());
        assert!(help_str.contains("D-Bus"));
    }

    #[test]
    fn test_io_error_conversion() {
        let io_err = std::io::Error::new(std::io::ErrorKind::NotFound, "not found");
        let app_err: AppError = io_err.into();
        matches!(app_err, AppError::Io(_));
    }

    #[test]
    fn test_log_and_context_ok() {
        let input: Result<i32, std::io::Error> = Ok(42);
        let result: Result<i32, anyhow::Error> = log_and_context(input, "test context");
        assert_eq!(result.unwrap(), 42);
    }

    #[test]
    fn test_log_and_context_err() {
        let result: Result<i32, anyhow::Error> = log_and_context(
            Err(std::io::Error::new(std::io::ErrorKind::Other, "fail")),
            "test context",
        );
        assert!(result.is_err());
    }
}
