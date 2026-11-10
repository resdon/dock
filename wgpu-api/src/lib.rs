//! # wgpu-api
//!
//! A hardware-accelerated 2D rendering API built on wgpu (Vulkan).
//!
//! ## Design Philosophy
//!
//! The public API is **independent of wgpu types**. Application code describes
//! **what** to render, not **how** to allocate GPU buffers, bind textures, or
//! submit command buffers. All wgpu internals (TextureView, RenderPipeline,
//! BindGroup, CommandEncoder, Buffer) are hidden inside the backend.
//!
//! ## Architecture
//!
//! ```text
//! ┌─────────────────────────────────────────────────────────────┐
//! │  Application Code (wgpu-api public types only)              │
//! │  Renderer, Surface, Texture, Path, Color, Transform...      │
//! ├─────────────────────────────────────────────────────────────┤
//! │  wgpu-api Backend                                           │
//! │  ┌──────────┐  ┌──────────┐  ┌──────────┐  ┌────────────┐ │
//! │  │ Geometry │  │ Primitives│  │ Text     │  │ Animation  │ │
//! │  │ Compiler │  │ Compiler │  │ Rasterizer│ │ Engine     │ │
//! │  └────┬─────┘  └────┬─────┘  └────┬─────┘  └─────┬──────┘ │
//! │       └─────────────┴─────────────┴──────────────┘         │
//! │                         ▼                                   │
//! │              ┌──────────────────────┐                       │
//! │              │   Command Encoder    │                       │
//! │              │   (wgpu::CommandBuf) │                       │
//! │              └──────────┬───────────┘                       │
//! │                         ▼                                   │
//! │              ┌──────────────────────┐                       │
//! │              │   GPU Submit + Sync  │                       │
//! │              └──────────────────────┘                       │
//! └─────────────────────────────────────────────────────────────┘
//! ```
//!
//! ## Core Types
//!
//! - [`Renderer`] — GPU device, queue, pipelines, and caches
//! - [`SurfaceId`] / [`TextureId`] / [`NodeId`] / [`AnimationId`] — opaque handles
//! - [`Size`], [`Point`], [`Rect`], [`Vector2D`] — floating-point geometry
//! - [`Color`] — RGBA color with alpha
//! - [`Path`] — vector path for shapes and strokes
//! - [`Transform`] — 2D affine transformation matrix
//! - [`Gradient`] — linear and radial gradients
//! - [`Image`] — raster image with filtering
//! - [`Text`] — text layout and rendering
//! - [`Animation`] — animated properties with easing
//!
//! ## Example
//!
//! ```no_run
//! use wgpu_api::{Renderer, Color, Rect, Path, Transform};
//!
//! # async fn example() {
//! let mut renderer = Renderer::new().await.unwrap();
//! let surface = renderer.create_surface(800, 600).await.unwrap();
//!
//! renderer.with_frame(surface, |cmd| {
//!     cmd.clear(Color::rgba(0.1, 0.1, 0.15, 1.0));
//!
//!     let mut path = Path::new();
//!     path.rounded_rect(50.0, 50.0, 200.0, 100.0, 12.0);
//!     cmd.fill(&path, &Color::rgb(0.2, 0.6, 1.0));
//!
//!     cmd.transform(&Transform::translate(300.0, 100.0));
//!     cmd.text("Hello, wgpu-api!", 16.0, &Color::WHITE);
//! });
//! # }
//! ```

// ---------------------------------------------------------------------------
// Module declarations
// ---------------------------------------------------------------------------

pub mod geometry;
pub mod color;
pub mod primitives;
pub mod transform;
pub mod image;
pub mod text;
pub mod animation;
pub mod surface;
pub mod texture;
pub mod renderer;
pub mod command;
pub mod layout;
pub mod brush;
pub mod frame;
pub mod scene;
pub mod render;
pub mod shaders;
pub mod backend;
pub mod glyph;
pub mod shaping;
pub mod fontdb;
pub mod video;
pub mod viewport;
pub mod morphing;
pub mod clip;
pub mod wayland;
pub mod slint;
pub mod gpu;
pub mod batching;
pub mod gif;
pub mod math_utils;
pub mod monitor;
pub mod perf;
pub mod physics;
pub mod error;

// ---------------------------------------------------------------------------
// Re-export core types at the crate root for ergonomic imports
// ---------------------------------------------------------------------------

// Core identifiers
pub use geometry::{Size, Point, Rect, Vector2D};
pub use color::{Color, Gradient, GradientType, BlendMode};
pub use primitives::{Path, Shape, StrokeStyle, FillRule, LineCap, LineJoin, DashPattern};
pub use transform::{Transform, Matrix3};
pub use image::{Image, ImageFormat, ImageFilter, ImageData};
pub use text::{Text, TextStyle, TextAlign, TextBaseline, Font, FontWeight, FontStyle};
pub use animation::{
    Animation, AnimationType, Easing, AnimationId, AnimationState,
    AnimationRepeat, AnimationValue,
    PulseAnimation, RotateAnimation, FadeAnimation, BounceAnimation, SpringAnimation,
    KeyframeAnimation, Keyframe,
    AnimatedProperty, ContinuousAnimation, ContinuousAnimationType,
    AnimatedNode, AnimationManager,
    InteractionMouseButton, MouseButtonState, InteractionEvent,
    HoverState, DragState, GestureType, GestureState,
    InteractionConfig, InteractionState,
    StarShape, PolygonShape, RingShape, ProgressRing,
    Particle, ParticleSystemConfig, ParticleSystem,
};
pub use surface::{Surface, SurfaceFormat, SurfaceId, SurfaceConfig, SurfaceDescriptor};
pub use texture::{Texture, TextureId, TextureFormat, TextureFilter, TextureWrap};
pub use renderer::{Renderer, RendererConfig, RendererBackend};
pub use command::{Command, CommandBuffer, FrameCallback};

// Opaque handle types (re-exported from their modules)
pub use geometry::NodeId;

// Layout and styling types
pub use layout::{Insets, CornerRadius, Shadow};
pub use brush::Brush;
pub use frame::{FrameInfo, PointerLocation, MouseButton, PointerEvent};

// Scene graph types
pub use scene::{
    NodeStyle, NodeContent, Node, Scene,
    Rectangle, SceneImage, SceneText, Group,
};

// Rendering pipeline types
pub use render::{
    RenderError, RenderTarget, DrawCommand, FrameState, RenderPassInfo,
};

// Glyph atlas and caching types
pub use glyph::{
    GlyphInfo, TextureRegion, GlyphCacheKey, GlyphCluster, GlyphRun,
    ShapedLine, GlyphAtlas, AtlasStats, GlyphCache,
    RasterizationMode,
};

// Text shaping pipeline types
pub use shaping::{
    ShapedText, ShaperConfig, FontFace, TextShaper,
    FontFallback, GlyphRasterizer, GlyphRaster,
};

// Font database types
pub use fontdb::{
    FontProperties, FontStretch, FontEntry, FontDatabase, FontResolver,
};

// Video and PipeWire integration types
pub use video::{
    StreamId, FrameHandle, VideoFormat, StreamState,
    VideoFrame, VideoStream, StreamStats,
    VideoTexture, VideoManager, VideoManagerStats,
    VideoNode, VideoFitMode,
    FrameBuffer, FrameBufferSource,
    convert_bgfx_to_rgba, convert_bgra_to_rgba, convert_rgb_to_rgba,
    estimate_video_vram, estimate_video_memory,
};

// Viewport and coordinate system types
pub use viewport::{
    Viewport, ResizeEvent, ResourceChanges, TextScalePolicy,
    compute_resource_changes,
};

// Geometry morphing and animation types
pub use morphing::{
    GeometryAnimation, SizeAnimation, PositionAnimation,
    MorphType, PopupAnimation, MorphManager,
};

// Clipping types
pub use clip::{
    ClipRect, ClipStack, RoundedClip, ScissorRect, ClipMode,
};

// Wayland client integration types
pub use wayland::{
    WaylandWindowId, WaylandPopupId,
    WindowState, WindowType,
    WaylandWindowConfig, WaylandWindow,
    PopupAnchor, PopupGravity,
    WaylandPopupConfig, WaylandPopup,
    KeyState, KeyEvent,
    PointerButtonState, PointerButton, PointerEventType,
    AxisType, ModifiersState,
    TouchId, TouchEventType, TouchEvent,
    InputEvent,
    WaylandConnection,
    VisualPopup, PopupShadow,
    OutputInfo, OutputTransform,
};

// Slint integration and widget types
pub use slint::{
    WidgetId,
    DockItem, DockWidget, DashConfig,
    DeviceEntry, BluetoothAction,
    BluetoothState, BluetoothWidget,
    SlintRenderMode, SlintIntegrationConfig,
    SlintComponent, WidgetRegistry,
    SlintEvent, SlintMouseButton,
};

// GPU backend types
pub use gpu::{
    QuadInstance, PipelineType, BlendMode as GpuBlendMode, PipelineKey,
    OrthographicProjection, GpuUniformBuffer,
    CachedTexture, TextureCache,
    CachedPipeline, PipelineCache,
    CachedBuffer, BufferType, BufferCache,
    CachedGlyph,
    GpuResources,
    SurfacePresentationState, SurfacePresentationConfig, SurfaceAlphaMode,
    SurfacePresenter, PresentationError,
    FrameStats, GpuBackendConfig,
};

// Batching types
pub use batching::{
    BatchKey, BatchedCommand, DrawCommandType,
    Batch, BatchSorter, BatchInfo,
    InstanceBufferBuilder, DrawCallEstimator,
};

// GIF decoding and animation types
pub use gif::{
    GifError, GifFrameInfo, GifDisposalMethod,
    GifInfo, GifDecoder, GifLoopCount,
    GifAnimation,
};

// Math utilities (glam integration)
pub use math_utils::{
    Vec2, Mat2, Mat3 as GlamMat3, Mat3A, Mat4, Quat, Vec3, Vec4,
    ToGlam, FromGlam,
    Vec2Ext, Mat2DExt,
    vec2_cross,
    orthographic_topleft, orthographic_topleft_a,
    orthographic_topleft_2d,
    transform_2d_mat3, transform_2d_mat3a, transform_2d_mat4,
    transform_points, transform_rects,
    smoothstep, damp, damp_vec2, catmull_rom,
    hash_f32, value_noise,
};

// Resource monitoring types
pub use monitor::{
    MemorySize,
    GpuMemoryBreakdown, GpuMemoryBudget, GpuMemoryUsage, GpuMemoryCategory,
    CpuMemoryBreakdown, CpuMemoryUsage, CpuMemoryCategory,
    CacheStats, AggregateCacheStats,
    SystemResourceLimits,
    ResourceSnapshot, ResourceHealth,
    FrameRenderStats,
    HistoryEntry, ResourceHistory, HistorySummary,
    ResourceMonitor,
};

// Performance profiling types
pub use perf::{
    Stopwatch, TimingScope,
    RenderPhase, PhaseTiming,
    FrameRateAnalyzer, FrameRateData,
    PerfCounter,
    ProfileSnapshot,
    Profiler,
    StressTestConfig, StressTestResult,
    JitterAnalyzer,
};

// Physics engine types
pub use physics::{
    // Core
    BodyId, BodyType, PhysicsBody, CollisionShape, DampingConfig, MaterialProperties,
    PhysicalProperties, PhysicsObject,
    PhysicsWorld, PhysicsWorldConfig, PhysicsStats,
    // System A: Springs
    SpringConstraint as PhysicsSpringConstraint, DistanceConstraint, SpringChain, RadialSpringSystem,
    SpringForce, DragForce, RepulsionForce, AttractionForce,
    WindForce, PushForce, BodyRepulsion, ForceAccumulator,
    apply_rest_springs, apply_mutual_repulsion,
    // System B: Rigid body
    Collision, CollisionPair, broad_phase, detect_collision, resolve_collision,
    PhysicsSolver, SolverConfig, SolverStats, ContactManifold, ContactPoint,
    AnchorConstraint, HingeJoint, WeldJoint, PrismaticJoint,
    RegionConstraint, SliderConstraint, ConstraintSet,
    // Constraint system (new)
    FixedConstraint, ConstraintsDistanceConstraint,
    AxisConstraint, PathConstraint, AlignmentConstraint, AlignmentAxis,
    SeparationConstraint, AttractionConstraint as PhysicsAttractionConstraint, GroupConstraint,
    Constraint, ConstraintSystem,
    // System C: Fields
    ForceField, FieldType, FieldConfig, FieldSource,
    MagneticField, CohesionField, SeparationField,
    RadialField, DirectionalField,
    // Groups and spatial
    BodyGroup, GroupId, GroupConfig, GroupBehavior, GroupManager,
    SpatialHash, SpatialHashConfig, SpatialHashStats,
    // High-level interaction API
    InteractionApi, PhysicsDragState, DragConfig, ProjectionAxis,
    InsertResult, CutResult, MergeResult,
    // Physical dock layout model
    Dock as PhysicsDock, DockItem as PhysicsDockItem, DockConfig as PhysicsDockConfig, DockAxis as PhysicsDockAxis, DockDragState as PhysicsDockDragState,
    // Visual effects system
    MotionState, VisualTransform, VisualEffectsConfig,
    CollisionEffect, DragVisualState, VisualEffects,
    GroupVisualEffects,
    // Fixed timestep simulation
    PhysicsConfig, FixedTimestepSimulator, InterpolatedState,
    // Interaction command system
    InteractionCommand, InteractionManager, CommandResult,
    KeyboardKey, KeyboardModifiers, TouchPhase, PhysicsGestureType,
    ScriptParameter,
};

// Error handling types
pub use error::{
    DockError, DockResult,
    GpuError, RendererError, PhysicsError,
    VideoError, WaylandError, SceneError,
    LayoutError, TextError, InteractionError,
    AnimationError,
    log_err, log_warn,
};
