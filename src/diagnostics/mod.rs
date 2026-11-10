//! Diagnostics subsystem for Dock.
//!
//! # Purpose
//! Provides tools for monitoring, logging, and diagnosing issues across
//! all subsystems: GPU, Wayland, PipeWire, UI, and system integration.
//!
//! # Design
//! - `logger`: Structured logging with subsystem-specific filters.
//! - `crash_report`: Crash report generation with backtraces and context.
//! - `performance`: Performance metrics collection and reporting.
//! - `gpu_validation`: GPU error scope monitoring and validation.
//! - `health_monitor`: Periodic health checks across all subsystems.
//!
//! # Dependencies
//! - `tracing`: Structured logging backend.
//! - `wgpu_api::error`: Subsystem-specific error types.

mod crash_report;
mod gpu_validation;
mod health_monitor;
mod logger;
mod performance;

pub use crash_report::*;
pub use gpu_validation::*;
pub use health_monitor::*;
pub use logger::*;
pub use performance::*;
