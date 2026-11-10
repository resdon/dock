/// Error handling subsystem for wgpu-api.
///
/// # Purpose
/// Provides strongly-typed, subsystem-specific error types using `thiserror`,
/// with `anyhow` for application-level propagation and `miette` for human-readable
/// diagnostics. This module is the single source of truth for error types in wgpu-api.
///
/// # Design
/// - `thiserror`: Define strongly-typed errors for each subsystem.
/// - `anyhow`: Propagate errors at the application level with context.
/// - `tracing`: Record what happened before/after errors.
/// - `miette`: Present understandable diagnostics to developers.
///
/// # Subsystem Boundaries
/// Each subsystem defines its own error type. Cross-subsystem errors are wrapped
/// in `DockError` which serves as the unified error type for the wgpu-api crate.
///
/// # Coordinate Space
/// N/A — error types do not operate in coordinate space.
///
/// # Ownership
/// Error types are created by the subsystem that encounters the error and propagated
/// upward. The application layer wraps them in `anyhow::Result` for context.
use thiserror::Error;

// ---------------------------------------------------------------------------
// Unified wgpu-api error type
// ---------------------------------------------------------------------------

/// The top-level error type for the wgpu-api crate.
///
/// Wraps all subsystem-specific errors into a single enum so that callers
/// outside wgpu-api have one error type to handle.
///
/// # Examples
/// ```
/// use wgpu_api::DockError;
///
/// fn do_something() -> Result<(), DockError> {
///     // Subsystem errors are automatically converted via From impls
///     Ok(())
/// }
/// ```
#[derive(Debug, Error)]
pub enum DockError {
    #[error("GPU error: {0}")]
    Gpu(#[from] GpuError),

    #[error("Renderer error: {0}")]
    Renderer(#[from] RendererError),

    #[error("Physics error: {0}")]
    Physics(#[from] PhysicsError),

    #[error("Video/PipeWire error: {0}")]
    Video(#[from] VideoError),

    #[error("Wayland error: {0}")]
    Wayland(#[from] WaylandError),

    #[error("Scene error: {0}")]
    Scene(#[from] SceneError),

    #[error("Layout error: {0}")]
    Layout(#[from] LayoutError),

    #[error("Text error: {0}")]
    Text(#[from] TextError),

    #[error("Interaction error: {0}")]
    Interaction(#[from] InteractionError),

    #[error("Animation error: {0}")]
    Animation(#[from] AnimationError),

    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
}

// ---------------------------------------------------------------------------
// GPU Error
// ---------------------------------------------------------------------------

/// Errors related to GPU operations.
///
/// # Ownership
/// Created by the GPU backend when device, queue, or buffer operations fail.
///
/// # Dependencies
/// This error type is used by `gpu/` and `renderer/` subsystems.
#[derive(Debug, Error)]
pub enum GpuError {
    #[error("GPU device lost: {0}")]
    DeviceLost(String),

    #[error("GPU device creation failed: {0}")]
    DeviceCreationFailed(String),

    #[error("GPU instance creation failed: {0}")]
    InstanceCreationFailed(String),

    #[error("GPU surface configuration failed: {0}")]
    SurfaceConfigurationFailed(String),

    #[error("GPU texture creation failed: {0}")]
    TextureCreationFailed(String),

    #[error("GPU buffer allocation failed: {0}")]
    BufferAllocationFailed(String),

    #[error("GPU pipeline compilation failed: {0}")]
    PipelineCompilationFailed(String),

    #[error("GPU command encoding failed: {0}")]
    CommandEncodingFailed(String),

    #[error("GPU submission error: {0}")]
    SubmissionError(String),

    #[error("GPU synchronization timeout: {0}")]
    SyncTimeout(String),

    #[error("GPU out of memory")]
    OutOfMemory,

    #[error("DMA-BUF import failed: {0}")]
    DmaBufImportFailed(String),

    #[error("GPU feature not supported: {0}")]
    FeatureNotSupported(String),
}

// ---------------------------------------------------------------------------
// Renderer Error
// ---------------------------------------------------------------------------

/// Errors related to the rendering subsystem.
///
/// # Ownership
/// Created by the renderer when frame rendering, texture management,
/// or surface operations fail.
///
/// # Dependencies
/// Depends on `GpuError` for low-level GPU failures.
#[derive(Debug, Error)]
pub enum RendererError {
    #[error("Renderer initialization failed: {0}")]
    InitializationFailed(String),

    #[error("Surface recreation failed: {0}")]
    SurfaceRecreationFailed(String),

    #[error("Frame rendering failed: {0}")]
    FrameRenderingFailed(String),

    #[error("Texture upload failed: {0}")]
    TextureUploadFailed(String),

    #[error("Texture not found: {id}")]
    TextureNotFound { id: u32 },

    #[error("Render target mismatch: expected ({expected_w}, {expected_h}), got ({actual_w}, {actual_h})")]
    RenderTargetMismatch {
        expected_w: u32,
        expected_h: u32,
        actual_w: u32,
        actual_h: u32,
    },

    #[error("Renderer resize failed: {0}")]
    ResizeFailed(String),

    #[error("Presentation failed: {0}")]
    PresentationFailed(String),

    #[error("Shader compilation failed: {0}")]
    ShaderCompilationFailed(String),

    #[error("Render pipeline not found: {name}")]
    PipelineNotFound { name: String },
}

// ---------------------------------------------------------------------------
// Physics Error
// ---------------------------------------------------------------------------

/// Errors related to the physics engine.
///
/// # Ownership
/// Created by the physics subsystem when simulation, constraint resolution,
/// or body management fails.
///
/// # Invariants
/// Physics errors should never be caused by wgpu or Wayland issues.
/// The physics engine is pure Rust.
#[derive(Debug, Error)]
pub enum PhysicsError {
    #[error("Body not found: {id}")]
    BodyNotFound { id: u32 },

    #[error("Body already exists: {id}")]
    BodyAlreadyExists { id: u32 },

    #[error("Constraint resolution failed after {iterations} iterations")]
    ConstraintResolutionFailed { iterations: u32 },

    #[error("Physics simulation step failed: {0}")]
    SimulationStepFailed(String),

    #[error("Invalid physics configuration: {0}")]
    InvalidConfiguration(String),

    #[error("Spring solver divergence: stiffness={stiffness}, damping={damping}")]
    SpringSolverDivergence { stiffness: f32, damping: f32 },

    #[error("Constraint not found: {id}")]
    ConstraintNotFound { id: u32 },

    #[error("Group not found: {id}")]
    GroupNotFound { id: u32 },

    #[error("Invalid body type for operation: {0}")]
    InvalidBodyType(String),

    #[error("Physics world is not active")]
    WorldNotActive,

    #[error("Timestep too large: {dt} (max: {max_dt})")]
    TimestepTooLarge { dt: f32, max_dt: f32 },
}

// ---------------------------------------------------------------------------
// Video/PipeWire Error
// ---------------------------------------------------------------------------

/// Errors related to video capture and PipeWire integration.
///
/// # Ownership
/// Created by the video subsystem when stream management, frame acquisition,
/// or buffer handling fails.
///
/// # Invariants
/// PipeWire buffers must be released promptly. Errors related to buffer
/// lifetime violations are critical.
#[derive(Debug, Error)]
pub enum VideoError {
    #[error("PipeWire context creation failed: {0}")]
    ContextCreationFailed(String),

    #[error("PipeWire connection failed: {0}")]
    ConnectionFailed(String),

    #[error("Stream creation failed: {0}")]
    StreamCreationFailed(String),

    #[error("Stream negotiation failed: {0}")]
    StreamNegotiationFailed(String),

    #[error("Frame acquisition timeout")]
    FrameAcquisitionTimeout,

    #[error("Buffer release failed: {0}")]
    BufferReleaseFailed(String),

    #[error("Buffer format mismatch: expected {expected}, got {actual}")]
    BufferFormatMismatch { expected: String, actual: String },

    #[error("DMA-BUF capability not negotiated")]
    DmaBufNotNegotiated,

    #[error("Stream already active")]
    StreamAlreadyActive,

    #[error("Stream not found: {id}")]
    StreamNotFound { id: u32 },

    #[error("No active stream")]
    NoActiveStream,

    #[error("Frame upload failed: {0}")]
    FrameUploadFailed(String),

    #[error("PipeWire main loop error: {0}")]
    MainLoopError(String),

    #[error("SPA pod error: {0}")]
    SpaPodError(String),
}

// ---------------------------------------------------------------------------
// Wayland Error
// ---------------------------------------------------------------------------

/// Errors related to Wayland compositor integration.
///
/// # Ownership
/// Created by the Wayland subsystem when window creation, event handling,
/// or protocol communication fails.
///
/// # Dependencies
/// Wayland errors may cascade to renderer errors if surface operations fail.
#[derive(Debug, Error)]
pub enum WaylandError {
    #[error("Wayland connection failed: {0}")]
    ConnectionFailed(String),

    #[error("Wayland display not found")]
    DisplayNotFound,

    #[error("Surface creation failed: {0}")]
    SurfaceCreationFailed(String),

    #[error("Surface not found: {id}")]
    SurfaceNotFound { id: u32 },

    #[error("Protocol error: {0}")]
    ProtocolError(String),

    #[error("Global not found: {name}")]
    GlobalNotFound { name: String },

    #[error("Layer shell not supported")]
    LayerShellNotSupported,

    #[error("Event queue error: {0}")]
    EventQueueError(String),

    #[error("Wayland roundtrip failed: {0}")]
    RoundtripFailed(String),

    #[error("Output configuration error: {0}")]
    OutputError(String),

    #[error("Scale factor query failed")]
    ScaleFactorError,
}

// ---------------------------------------------------------------------------
// Scene Error
// ---------------------------------------------------------------------------

/// Errors related to the scene graph.
///
/// # Ownership
/// Created when scene node operations fail.
#[derive(Debug, Error)]
pub enum SceneError {
    #[error("Node not found: {id}")]
    NodeNotFound { id: u32 },

    #[error("Node already exists: {id}")]
    NodeAlreadyExists { id: u32 },

    #[error("Parent node not found: {id}")]
    ParentNotFound { id: u32 },

    #[error("Cannot reparent root node")]
    CannotReparentRoot,

    #[error("Scene traversal error: {0}")]
    TraversalError(String),

    #[error("Cycle detected in scene graph")]
    CycleDetected,
}

// ---------------------------------------------------------------------------
// Layout Error
// ---------------------------------------------------------------------------

/// Errors related to dock layout and arrangement.
///
/// # Ownership
/// Created when layout calculations or dock operations fail.
#[derive(Debug, Error)]
pub enum LayoutError {
    #[error("Layout calculation failed: {0}")]
    CalculationFailed(String),

    #[error("Dock item not found: {id}")]
    ItemNotFound { id: u32 },

    #[error("Invalid insertion index: {index} (max: {max})")]
    InvalidInsertionIndex { index: usize, max: usize },

    #[error("Layout axis mismatch: expected {expected}, got {actual}")]
    AxisMismatch { expected: String, actual: String },

    #[error("Projection failed: {0}")]
    ProjectionFailed(String),
}

// ---------------------------------------------------------------------------
// Text Error
// ---------------------------------------------------------------------------

/// Errors related to text shaping and font rendering.
///
/// # Ownership
/// Created when text layout, font loading, or glyph rendering fails.
#[derive(Debug, Error)]
pub enum TextError {
    #[error("Font not found: {name}")]
    FontNotFound { name: String },

    #[error("Font loading failed: {0}")]
    FontLoadFailed(String),

    #[error("Text shaping failed: {0}")]
    ShapingFailed(String),

    #[error("Glyph not found: codepoint={codepoint}, font={font}")]
    GlyphNotFound { codepoint: char, font: String },

    #[error("Text layout overflow: width={width}, available={available}")]
    LayoutOverflow { width: f32, available: f32 },

    #[error("Font database error: {0}")]
    FontDatabaseError(String),
}

// ---------------------------------------------------------------------------
// Interaction Error
// ---------------------------------------------------------------------------

/// Errors related to input handling and interaction processing.
///
/// # Ownership
/// Created when input event processing or gesture recognition fails.
#[derive(Debug, Error)]
pub enum InteractionError {
    #[error("Target not found: {id}")]
    TargetNotFound { id: u32 },

    #[error("Invalid gesture state transition: {from} -> {to}")]
    InvalidGestureTransition { from: String, to: String },

    #[error("Input device error: {0}")]
    InputDeviceError(String),

    #[error("Hit test failed: {0}")]
    HitTestFailed(String),

    #[error("Command execution failed: {0}")]
    CommandExecutionFailed(String),

    #[error("Drag operation error: {0}")]
    DragError(String),
}

// ---------------------------------------------------------------------------
// Animation Error
// ---------------------------------------------------------------------------

/// Errors related to animation and easing.
///
/// # Ownership
/// Created when animation state transitions or keyframe interpolation fails.
#[derive(Debug, Error)]
pub enum AnimationError {
    #[error("Animation not found: {id}")]
    AnimationNotFound { id: u32 },

    #[error("Invalid easing function: {0}")]
    InvalidEasing(String),

    #[error("Keyframe interpolation failed: {0}")]
    KeyframeInterpolationFailed(String),

    #[error("Animation timeline error: {0}")]
    TimelineError(String),

    #[error("Animation already running: {id}")]
    AnimationAlreadyRunning { id: u32 },
}

// ---------------------------------------------------------------------------
// Result type alias
// ---------------------------------------------------------------------------

/// Result type alias for wgpu-api operations.
///
/// # Examples
/// ```
/// use wgpu_api::DockResult;
///
/// fn operation() -> DockResult<()> {
///     Ok(())
/// }
/// ```
pub type DockResult<T> = Result<T, DockError>;

// ---------------------------------------------------------------------------
// Miette integration for human-readable diagnostics
// ---------------------------------------------------------------------------

impl miette::Diagnostic for DockError {
    fn code<'a>(&'a self) -> Option<Box<dyn std::fmt::Display + 'a>> {
        Some(Box::new(match self {
            DockError::Gpu(_) => "GPU_ERROR",
            DockError::Renderer(_) => "RENDERER_ERROR",
            DockError::Physics(_) => "PHYSICS_ERROR",
            DockError::Video(_) => "VIDEO_ERROR",
            DockError::Wayland(_) => "WAYLAND_ERROR",
            DockError::Scene(_) => "SCENE_ERROR",
            DockError::Layout(_) => "LAYOUT_ERROR",
            DockError::Text(_) => "TEXT_ERROR",
            DockError::Interaction(_) => "INTERACTION_ERROR",
            DockError::Animation(_) => "ANIMATION_ERROR",
            DockError::Io(_) => "IO_ERROR",
        }))
    }

    fn severity(&self) -> Option<miette::Severity> {
        Some(match self {
            DockError::Gpu(
                GpuError::DeviceLost(_) | GpuError::OutOfMemory | GpuError::SubmissionError(_),
            ) => miette::Severity::Error,
            DockError::Video(
                VideoError::ConnectionFailed(_) | VideoError::ContextCreationFailed(_),
            ) => miette::Severity::Error,
            DockError::Wayland(
                WaylandError::ConnectionFailed(_) | WaylandError::DisplayNotFound,
            ) => miette::Severity::Error,
            _ => miette::Severity::Warning,
        })
    }

    fn help<'a>(&'a self) -> Option<Box<dyn std::fmt::Display + 'a>> {
        let msg = match self {
            DockError::Gpu(GpuError::DeviceLost(_)) => {
                Some("GPU device was lost. Try restarting the application. Check Vulkan driver status.")
            }
            DockError::Gpu(GpuError::OutOfMemory) => {
                Some("GPU ran out of memory. Reduce texture sizes or close other GPU applications.")
            }
            DockError::Video(VideoError::ConnectionFailed(_)) => {
                Some("Cannot connect to PipeWire. Ensure PipeWire is running: `systemctl --user status pipewire`")
            }
            DockError::Video(VideoError::DmaBufNotNegotiated) => {
                Some("DMA-BUF sharing not available. Falling back to CPU upload path.")
            }
            DockError::Wayland(WaylandError::DisplayNotFound) => {
                Some("No Wayland display found. Ensure you are running under a Wayland compositor.")
            }
            DockError::Wayland(WaylandError::LayerShellNotSupported) => {
                Some("Layer shell protocol not supported by your compositor. Try a different compositor or configure layer shell support.")
            }
            DockError::Physics(PhysicsError::SpringSolverDivergence { .. }) => {
                Some("Spring parameters cause instability. Reduce stiffness or increase damping.")
            }
            _ => None,
        };
        msg.map(|s| Box::new(s) as Box<dyn std::fmt::Display>)
    }
}

// ---------------------------------------------------------------------------
// Tracing integration
// ---------------------------------------------------------------------------

/// Log an error using tracing.
///
/// This helper integrates error propagation with structured logging,
/// ensuring that every error is recorded before it bubbles up.
/// Call this before returning an error to ensure it's logged.
///
/// # Examples
/// ```
/// use wgpu_api::error::log_err;
///
/// fn do_something() -> Result<(), wgpu_api::DockError> {
///     let result = some_operation();
///     if let Err(ref e) = result {
///         log_err(e, "operation failed");
///     }
///     result
/// }
///
/// fn some_operation() -> Result<(), wgpu_api::DockError> {
///     Ok(())
/// }
/// ```
pub fn log_err<E: std::fmt::Display + std::fmt::Debug>(err: &E, context: &str) {
    tracing::error!(error = %err, error.debug = ?err, context);
}

/// Log a warning using tracing.
///
/// Useful for recoverable errors that should be logged but not propagated.
pub fn log_warn<T: std::fmt::Debug>(value: &T, message: &str) {
    tracing::warn!(value = ?value, message);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_gpu_error_conversion() {
        let gpu_err = GpuError::DeviceLost("test".to_string());
        let dock_err = DockError::Gpu(gpu_err);
        assert!(dock_err.to_string().contains("GPU error"));
    }

    #[test]
    fn test_physics_error_conversion() {
        let physics_err = PhysicsError::BodyNotFound { id: 42 };
        let dock_err = DockError::Physics(physics_err);
        assert!(dock_err.to_string().contains("Body not found"));
    }

    #[test]
    fn test_video_error_conversion() {
        let video_err = VideoError::ConnectionFailed("timeout".to_string());
        let dock_err = DockError::Video(video_err);
        assert!(dock_err.to_string().contains("PipeWire connection failed"));
    }

    #[test]
    fn test_wayland_error_conversion() {
        let wayland_err = WaylandError::DisplayNotFound;
        let dock_err = DockError::Wayland(wayland_err);
        assert!(dock_err.to_string().contains("Wayland display not found"));
    }

    #[test]
    fn test_dock_error_to_anyhow() {
        let dock_err = DockError::Gpu(GpuError::OutOfMemory);
        let anyhow_err: Box<dyn std::error::Error + Send + Sync> = dock_err.into();
        assert!(anyhow_err.to_string().contains("GPU out of memory"));
    }

    #[test]
    fn test_miette_code() {
        let err = DockError::Physics(PhysicsError::BodyNotFound { id: 1 });
        let code = miette::Diagnostic::code(&err);
        assert!(code.is_some());
        assert!(format!("{}", code.unwrap()).contains("PHYSICS_ERROR"));
    }

    #[test]
    fn test_miette_severity_critical_gpu() {
        let err = DockError::Gpu(GpuError::DeviceLost("test".into()));
        assert!(matches!(
            miette::Diagnostic::severity(&err),
            Some(miette::Severity::Error)
        ));
    }

    #[test]
    fn test_miette_severity_warning() {
        let err = DockError::Layout(LayoutError::CalculationFailed("test".into()));
        assert!(matches!(
            miette::Diagnostic::severity(&err),
            Some(miette::Severity::Warning)
        ));
    }

    #[test]
    fn test_miette_help_gpu_lost() {
        let err = DockError::Gpu(GpuError::DeviceLost("test".into()));
        let help = miette::Diagnostic::help(&err);
        assert!(help.is_some());
        let help_str = format!("{}", help.unwrap());
        assert!(help_str.contains("GPU device was lost"));
    }

    #[test]
    fn test_miette_help_wayland() {
        let err = DockError::Wayland(WaylandError::DisplayNotFound);
        let help = miette::Diagnostic::help(&err);
        assert!(help.is_some());
        let help_str = format!("{}", help.unwrap());
        assert!(help_str.contains("Wayland display"));
    }

    #[test]
    fn test_io_error_conversion() {
        let io_err = std::io::Error::new(std::io::ErrorKind::NotFound, "file not found");
        let dock_err: DockError = io_err.into();
        matches!(dock_err, DockError::Io(_));
    }

    #[test]
    fn test_dock_result_ok() {
        let result: DockResult<i32> = Ok(42);
        assert_eq!(result.unwrap(), 42);
    }

    #[test]
    fn test_dock_result_err() {
        let result: DockResult<i32> = Err(DockError::Gpu(GpuError::OutOfMemory));
        assert!(result.is_err());
    }

    #[test]
    fn test_scene_error() {
        let err = DockError::Scene(SceneError::NodeNotFound { id: 99 });
        assert!(err.to_string().contains("Node not found"));
    }

    #[test]
    fn test_text_error() {
        let err = DockError::Text(TextError::FontNotFound {
            name: "MissingFont".into(),
        });
        assert!(err.to_string().contains("Font not found"));
    }

    #[test]
    fn test_interaction_error() {
        let err = DockError::Interaction(InteractionError::TargetNotFound { id: 5 });
        assert!(err.to_string().contains("Target not found"));
    }

    #[test]
    fn test_animation_error() {
        let err = DockError::Animation(AnimationError::AnimationNotFound { id: 3 });
        assert!(err.to_string().contains("Animation not found"));
    }
}
