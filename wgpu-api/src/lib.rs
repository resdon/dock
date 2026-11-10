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

pub mod animation;
pub mod backend;
pub mod batching;
pub mod brush;
pub mod clip;
pub mod color;
pub mod command;
pub mod error;
pub mod fontdb;
pub mod frame;
pub mod geometry;
pub mod gif;
pub mod glyph;
pub mod gpu;
pub mod image;
pub mod layout;
pub mod math_utils;
pub mod monitor;
pub mod morphing;
pub mod perf;
pub mod physics;
pub mod primitives;
pub mod render;
pub mod renderer;
pub mod scene;
pub mod shaders;
pub mod shaping;
pub mod slint;
pub mod surface;
pub mod text;
pub mod texture;
pub mod transform;
pub mod video;
pub mod viewport;
pub mod wayland;

// ---------------------------------------------------------------------------
// Re-export core types at the crate root for ergonomic imports
// ---------------------------------------------------------------------------

// Core identifiers
pub use animation::{
    AnimatedNode, AnimatedProperty, Animation, AnimationId, AnimationManager, AnimationRepeat,
    AnimationState, AnimationType, AnimationValue, BounceAnimation, ContinuousAnimation,
    ContinuousAnimationType, DragState, Easing, FadeAnimation, GestureState, GestureType,
    HoverState, InteractionConfig, InteractionEvent, InteractionMouseButton, InteractionState,
    Keyframe, KeyframeAnimation, MouseButtonState, Particle, ParticleSystem, ParticleSystemConfig,
    PolygonShape, ProgressRing, PulseAnimation, RingShape, RotateAnimation, SpringAnimation,
    StarShape,
};
pub use color::{BlendMode, Color, Gradient, GradientType};
pub use command::{Command, CommandBuffer, FrameCallback};
pub use geometry::{Point, Rect, Size, Vector2D};
pub use image::{Image, ImageData, ImageFilter, ImageFormat};
pub use primitives::{DashPattern, FillRule, LineCap, LineJoin, Path, Shape, StrokeStyle};
pub use renderer::{Renderer, RendererBackend, RendererConfig};
pub use surface::{Surface, SurfaceConfig, SurfaceDescriptor, SurfaceFormat, SurfaceId};
pub use text::{Font, FontStyle, FontWeight, Text, TextAlign, TextBaseline, TextStyle};
pub use texture::{Texture, TextureFilter, TextureFormat, TextureId, TextureWrap};
pub use transform::{Matrix3, Transform};

// Opaque handle types (re-exported from their modules)
pub use geometry::NodeId;

// Layout and styling types
pub use brush::Brush;
pub use frame::{FrameInfo, MouseButton, PointerEvent, PointerLocation};
pub use layout::{CornerRadius, Insets, Shadow};

// Scene graph types
pub use scene::{Group, Node, NodeContent, NodeStyle, Rectangle, Scene, SceneImage, SceneText};

// Rendering pipeline types
pub use render::{DrawCommand, FrameState, RenderError, RenderPassInfo, RenderTarget};

// Glyph atlas and caching types
pub use glyph::{
    AtlasStats, GlyphAtlas, GlyphCache, GlyphCacheKey, GlyphCluster, GlyphInfo, GlyphRun,
    RasterizationMode, ShapedLine, TextureRegion,
};

// Text shaping pipeline types
pub use shaping::{
    FontFace, FontFallback, GlyphRaster, GlyphRasterizer, ShapedText, ShaperConfig, TextShaper,
};

// Font database types
pub use fontdb::{FontDatabase, FontEntry, FontProperties, FontResolver, FontStretch};

// Video and PipeWire integration types
pub use video::{
    convert_bgfx_to_rgba, convert_bgra_to_rgba, convert_rgb_to_rgba, estimate_video_memory,
    estimate_video_vram, FrameBuffer, FrameBufferSource, FrameHandle, StreamId, StreamState,
    StreamStats, VideoFitMode, VideoFormat, VideoFrame, VideoManager, VideoManagerStats, VideoNode,
    VideoStream, VideoTexture,
};

// Viewport and coordinate system types
pub use viewport::{
    compute_resource_changes, ResizeEvent, ResourceChanges, TextScalePolicy, Viewport,
};

// Geometry morphing and animation types
pub use morphing::{
    GeometryAnimation, MorphManager, MorphType, PopupAnimation, PositionAnimation, SizeAnimation,
};

// Clipping types
pub use clip::{ClipMode, ClipRect, ClipStack, RoundedClip, ScissorRect};

// Wayland client integration types
pub use wayland::{
    AxisType, InputEvent, KeyEvent, KeyState, ModifiersState, OutputInfo, OutputTransform,
    PointerButton, PointerButtonState, PointerEventType, PopupAnchor, PopupGravity, PopupShadow,
    TouchEvent, TouchEventType, TouchId, VisualPopup, WaylandConnection, WaylandPopup,
    WaylandPopupConfig, WaylandPopupId, WaylandWindow, WaylandWindowConfig, WaylandWindowId,
    WindowState, WindowType,
};

// Slint integration and widget types
pub use slint::{
    BluetoothAction, BluetoothState, BluetoothWidget, DashConfig, DeviceEntry, DockItem,
    DockWidget, SlintComponent, SlintEvent, SlintIntegrationConfig, SlintMouseButton,
    SlintRenderMode, WidgetId, WidgetRegistry,
};

// GPU backend types
pub use gpu::{
    BlendMode as GpuBlendMode, BufferCache, BufferType, CachedBuffer, CachedGlyph, CachedPipeline,
    CachedTexture, FrameStats, GpuBackendConfig, GpuResources, GpuUniformBuffer,
    OrthographicProjection, PipelineCache, PipelineKey, PipelineType, PresentationError,
    QuadInstance, SurfaceAlphaMode, SurfacePresentationConfig, SurfacePresentationState,
    SurfacePresenter, TextureCache,
};

// Batching types
pub use batching::{
    Batch, BatchInfo, BatchKey, BatchSorter, BatchedCommand, DrawCallEstimator, DrawCommandType,
    InstanceBufferBuilder,
};

// GIF decoding and animation types
pub use gif::{
    GifAnimation, GifDecoder, GifDisposalMethod, GifError, GifFrameInfo, GifInfo, GifLoopCount,
};

// Math utilities (glam integration)
pub use math_utils::{
    catmull_rom, damp, damp_vec2, hash_f32, orthographic_topleft, orthographic_topleft_2d,
    orthographic_topleft_a, smoothstep, transform_2d_mat3, transform_2d_mat3a, transform_2d_mat4,
    transform_points, transform_rects, value_noise, vec2_cross, FromGlam, Mat2, Mat2DExt,
    Mat3 as GlamMat3, Mat3A, Mat4, Quat, ToGlam, Vec2, Vec2Ext, Vec3, Vec4,
};

// Resource monitoring types
pub use monitor::{
    AggregateCacheStats, CacheStats, CpuMemoryBreakdown, CpuMemoryCategory, CpuMemoryUsage,
    FrameRenderStats, GpuMemoryBreakdown, GpuMemoryBudget, GpuMemoryCategory, GpuMemoryUsage,
    HistoryEntry, HistorySummary, MemorySize, ResourceHealth, ResourceHistory, ResourceMonitor,
    ResourceSnapshot, SystemResourceLimits,
};

// Performance profiling types
pub use perf::{
    FrameRateAnalyzer, FrameRateData, JitterAnalyzer, PerfCounter, PhaseTiming, ProfileSnapshot,
    Profiler, RenderPhase, Stopwatch, StressTestConfig, StressTestResult, TimingScope,
};

// Physics engine types
pub use physics::{
    apply_mutual_repulsion,
    apply_rest_springs,
    broad_phase,
    detect_collision,
    resolve_collision,
    AlignmentAxis,
    AlignmentConstraint,
    AnchorConstraint,
    AttractionConstraint as PhysicsAttractionConstraint,
    AttractionForce,
    AxisConstraint,
    // Groups and spatial
    BodyGroup,
    // Core
    BodyId,
    BodyRepulsion,
    BodyType,
    CohesionField,
    // System B: Rigid body
    Collision,
    CollisionEffect,
    CollisionPair,
    CollisionShape,
    CommandResult,
    Constraint,
    ConstraintSet,
    ConstraintSystem,
    ConstraintsDistanceConstraint,
    ContactManifold,
    ContactPoint,
    CutResult,
    DampingConfig,
    DirectionalField,
    DistanceConstraint,
    // Physical dock layout model
    Dock as PhysicsDock,
    DockAxis as PhysicsDockAxis,
    DockConfig as PhysicsDockConfig,
    DockDragState as PhysicsDockDragState,
    DockItem as PhysicsDockItem,
    DragConfig,
    DragForce,
    DragVisualState,
    FieldConfig,
    FieldSource,
    FieldType,
    // Constraint system (new)
    FixedConstraint,
    FixedTimestepSimulator,
    ForceAccumulator,
    // System C: Fields
    ForceField,
    GroupBehavior,
    GroupConfig,
    GroupConstraint,
    GroupId,
    GroupManager,
    GroupVisualEffects,
    HingeJoint,
    InsertResult,
    // High-level interaction API
    InteractionApi,
    // Interaction command system
    InteractionCommand,
    InteractionManager,
    InterpolatedState,
    KeyboardKey,
    KeyboardModifiers,
    MagneticField,
    MaterialProperties,
    MergeResult,
    // Visual effects system
    MotionState,
    PathConstraint,
    PhysicalProperties,
    PhysicsBody,
    // Fixed timestep simulation
    PhysicsConfig,
    PhysicsDragState,
    PhysicsGestureType,
    PhysicsObject,
    PhysicsSolver,
    PhysicsStats,
    PhysicsWorld,
    PhysicsWorldConfig,
    PrismaticJoint,
    ProjectionAxis,
    PushForce,
    RadialField,
    RadialSpringSystem,
    RegionConstraint,
    RepulsionForce,
    ScriptParameter,
    SeparationConstraint,
    SeparationField,
    SliderConstraint,
    SolverConfig,
    SolverStats,
    SpatialHash,
    SpatialHashConfig,
    SpatialHashStats,
    SpringChain,
    // System A: Springs
    SpringConstraint as PhysicsSpringConstraint,
    SpringForce,
    TouchPhase,
    VisualEffects,
    VisualEffectsConfig,
    VisualTransform,
    WeldJoint,
    WindForce,
};

// Error handling types
pub use error::{
    log_err, log_warn, AnimationError, DockError, DockResult, GpuError, InteractionError,
    LayoutError, PhysicsError, RendererError, SceneError, TextError, VideoError, WaylandError,
};
