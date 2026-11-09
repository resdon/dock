//! Resolution pipeline.
//!
//! Orchestrates the full resolution chain: app → desktop → icon → window.

use std::collections::HashMap;

use sysinfo::System;

use crate::events::WindowEvent;
use crate::models::system::{
    AppId, ApplicationRecord, ProcessId, ProcessRecord, ResolutionConfidence, ResolutionEvidence,
    WindowId,
};

use super::application::ApplicationResolver;
use super::capabilities::CapabilityResolver;
use super::context::{ResolutionRequest, ResolverSnapshot, WindowHints};
use super::distribution::DistributionResolver;
use super::environment::EnvironmentResolver;
use super::icon_stage::IconResolver;
use super::process::ProcessResolver;
use super::window::WindowResolver;

#[derive(Clone, Debug)]
pub enum ResolutionAction {
    None,
    Upsert {
        window: crate::models::system::WindowRecord,
        application: ApplicationRecord,
        application_confidence: ResolutionConfidence,
        process: Option<(ProcessRecord, ResolutionConfidence)>,
    },
    Remove {
        window_id: WindowId,
    },
}

pub trait ResolverStage {
    fn name(&self) -> &'static str;
}

#[derive(Debug, Default, Clone, Copy)]
pub struct ProcessResolverStage {
    pub resolver: ProcessResolver,
}
impl ResolverStage for ProcessResolverStage {
    fn name(&self) -> &'static str {
        "process"
    }
}

#[derive(Debug, Default, Clone, Copy)]
pub struct ApplicationResolverStage {
    pub resolver: ApplicationResolver,
}
impl ResolverStage for ApplicationResolverStage {
    fn name(&self) -> &'static str {
        "application"
    }
}

#[derive(Debug, Default, Clone, Copy)]
pub struct EnvironmentResolverStage {
    pub resolver: EnvironmentResolver,
}
impl ResolverStage for EnvironmentResolverStage {
    fn name(&self) -> &'static str {
        "environment"
    }
}

#[derive(Debug, Default, Clone, Copy)]
pub struct CapabilityResolverStage {
    pub resolver: CapabilityResolver,
}
impl ResolverStage for CapabilityResolverStage {
    fn name(&self) -> &'static str {
        "capabilities"
    }
}

#[derive(Debug, Default, Clone, Copy)]
pub struct IconResolverStage {
    pub resolver: IconResolver,
}
impl ResolverStage for IconResolverStage {
    fn name(&self) -> &'static str {
        "icon"
    }
}

#[derive(Debug, Default, Clone, Copy)]
pub struct DistributionResolverStage {
    pub resolver: DistributionResolver,
}
impl ResolverStage for DistributionResolverStage {
    fn name(&self) -> &'static str {
        "distribution"
    }
}

#[derive(Debug, Default, Clone, Copy)]
pub struct ResolverStages {
    pub process: ProcessResolverStage,
    pub application: ApplicationResolverStage,
    pub environment: EnvironmentResolverStage,
    pub capabilities: CapabilityResolverStage,
    pub icon: IconResolverStage,
    pub distribution: DistributionResolverStage,
    pub window: WindowResolver,
}

#[derive(Clone, Debug)]
struct CachedApplication {
    record: ApplicationRecord,
    confidence: ResolutionConfidence,
}

#[derive(Clone, Debug)]
struct CachedProcess {
    record: ProcessRecord,
    confidence: ResolutionConfidence,
}

/// Orchestrates independent resolver stages. It owns caches and transient
/// window hints, but has no dependency on AppState, rendering, or Wayland
/// protocol objects beyond the stable domain IDs carried by events.
#[derive(Debug, Default)]
pub struct ResolverPipeline {
    pub stages: ResolverStages,
    windows: HashMap<WindowId, WindowHints>,
    applications: HashMap<AppId, CachedApplication>,
    processes: HashMap<ProcessId, CachedProcess>,
    /// Most recent process snapshot. Reused between throttled scans so a Done
    /// event never forces an empty-process resolution just because the scan
    /// interval has not elapsed yet.
    last_snapshot: ResolverSnapshot,
}

impl ResolverPipeline {
    pub fn invalidate_application(&mut self, app_id: &AppId) {
        self.applications.remove(app_id);
    }

    pub fn invalidate_process(&mut self, pid: ProcessId) {
        self.processes.remove(&pid);
    }

    pub fn update_snapshot(&mut self, snapshot: ResolverSnapshot) {
        self.last_snapshot = snapshot;
    }

    pub fn last_snapshot(&self) -> &ResolverSnapshot {
        &self.last_snapshot
    }

    pub fn handle_window_event_with_system(
        &mut self,
        event: WindowEvent,
        system: &System,
    ) -> ResolutionAction {
        let snapshot = ResolverSnapshot::from_system(system);
        self.update_snapshot(snapshot);
        let snapshot = self.last_snapshot.clone();
        self.handle_window_event(event, &snapshot)
    }

    pub fn handle_window_event(
        &mut self,
        event: WindowEvent,
        snapshot: &ResolverSnapshot,
    ) -> ResolutionAction {
        match event {
            WindowEvent::Created { window_id } => {
                self.windows.entry(window_id).or_default().dirty = true;
                ResolutionAction::None
            }
            WindowEvent::TitleChanged { window_id, title } => {
                let hints = self.windows.entry(window_id).or_default();
                hints.title = title;
                hints.dirty = true;
                ResolutionAction::None
            }
            WindowEvent::AppIdChanged { window_id, app_id } => {
                let hints = self.windows.entry(window_id).or_default();
                hints.app_id = Some(app_id);
                hints.dirty = true;
                ResolutionAction::None
            }
            WindowEvent::StateChanged {
                window_id,
                activated,
                minimized,
            } => {
                let hints = self.windows.entry(window_id).or_default();
                hints.activated = activated;
                hints.minimized = minimized;
                hints.dirty = true;
                ResolutionAction::None
            }
            WindowEvent::OutputEntered { window_id, output } => {
                let hints = self.windows.entry(window_id).or_default();
                if !hints.outputs.contains(&output) {
                    hints.outputs.push(output);
                }
                hints.dirty = true;
                ResolutionAction::None
            }
            WindowEvent::OutputLeft { window_id, output } => {
                let hints = self.windows.entry(window_id).or_default();
                hints.outputs.retain(|id| id != &output);
                hints.dirty = true;
                ResolutionAction::None
            }
            WindowEvent::Done { window_id } => self.resolve_window(window_id, snapshot),
            WindowEvent::Closed { window_id } => {
                self.windows.remove(&window_id);
                ResolutionAction::Remove { window_id }
            }
        }
    }

    pub fn request_for(
        &self,
        window_id: WindowId,
        snapshot: ResolverSnapshot,
    ) -> Option<ResolutionRequest> {
        self.windows
            .get(&window_id)
            .cloned()
            .map(|hints| ResolutionRequest {
                window_id,
                hints,
                snapshot,
            })
    }

    pub fn resolve_request(&mut self, request: ResolutionRequest) -> ResolutionAction {
        self.resolve_from_hints(request.window_id, request.hints, &request.snapshot)
    }

    fn resolve_window(
        &mut self,
        window_id: WindowId,
        snapshot: &ResolverSnapshot,
    ) -> ResolutionAction {
        let Some(hints) = self.windows.get(&window_id).cloned() else {
            return ResolutionAction::None;
        };
        if !hints.dirty {
            return ResolutionAction::None;
        }
        self.resolve_from_hints(window_id, hints, snapshot)
    }

    fn resolve_from_hints(
        &mut self,
        window_id: WindowId,
        hints: WindowHints,
        snapshot: &ResolverSnapshot,
    ) -> ResolutionAction {
        let raw_app_id = hints
            .app_id
            .clone()
            .filter(|id| !id.as_str().trim().is_empty())
            .unwrap_or_else(|| {
                AppId::from(if hints.title.trim().is_empty() {
                    "unknown"
                } else {
                    &hints.title
                })
            });

        let (mut application, mut app_confidence) =
            if let Some(cached) = self.applications.get(&raw_app_id) {
                (cached.record.clone(), cached.confidence)
            } else {
                let result = self
                    .stages
                    .application
                    .resolver
                    .resolve(&raw_app_id, &hints.title);
                self.applications.insert(
                    raw_app_id.clone(),
                    CachedApplication {
                        record: result.0.clone(),
                        confidence: result.1,
                    },
                );
                result
            };

        // Process resolution is independent from application resolution. It is
        // allowed to strengthen the application record but never required for it.
        let process = self
            .stages
            .process
            .resolver
            .find_candidate(raw_app_id.as_str(), &hints.title, snapshot)
            .and_then(|(pid, candidate_confidence)| {
                if let Some(cached) = self.processes.get_mut(&pid) {
                    if !self
                        .stages
                        .process
                        .resolver
                        .observe_cached(&mut cached.record, snapshot)
                    {
                        return None;
                    }
                    cached.record.app_id = Some(application.id.clone());
                    cached.confidence = cached.confidence.max(candidate_confidence);
                    Some((cached.record.clone(), cached.confidence))
                } else {
                    self.stages.process.resolver.resolve_candidate(
                        pid,
                        candidate_confidence,
                        snapshot,
                        &application,
                    )
                }
            })
            .map(|(mut record, confidence)| {
                let (runtime, packaging, environment_trace) = self
                    .stages
                    .environment
                    .resolver
                    .resolve_with_trace(record.binary.path.as_deref(), &record.env_vars);
                record.runtime = runtime;
                record.packaging = packaging;
                record.resolution_trace.merge(&environment_trace);

                let (distribution, distribution_trace) = self
                    .stages
                    .distribution
                    .resolver
                    .resolve_with_trace(record.binary.path.as_deref(), &record.env_vars);
                record.distribution = distribution;
                record.resolution_trace.merge(&distribution_trace);

                let final_confidence = confidence.max(record.resolution_trace.confidence);
                record.resolution_confidence = final_confidence;
                record.resolution_trace.confidence = final_confidence;
                (record, final_confidence)
            });

        if let Some((process_record, process_confidence)) = process.as_ref() {
            application.binary = process_record.binary.clone();
            application.platform = process_record.platform.clone();
            application.runtime = process_record.runtime.clone();
            application.packaging = process_record.packaging.clone();
            application.distribution = process_record.distribution.clone();
            application
                .resolution_trace
                .merge_evidence(&process_record.resolution_trace);
            application
                .resolution_trace
                .strengthen(*process_confidence, [ResolutionEvidence::ProcessId]);
            app_confidence = application.resolution_trace.confidence.max(app_confidence);
        }

        self.stages.icon.resolver.resolve(&mut application);
        self.stages.capabilities.resolver.resolve(&mut application);
        application.resolution_confidence =
            application.resolution_trace.confidence.max(app_confidence);
        application.resolution_trace.confidence = application.resolution_confidence;
        self.applications.insert(
            raw_app_id.clone(),
            CachedApplication {
                record: application.clone(),
                confidence: application.resolution_confidence,
            },
        );

        if let Some((process_record, confidence)) = process.as_ref() {
            self.processes.insert(
                process_record.pid,
                CachedProcess {
                    record: process_record.clone(),
                    confidence: *confidence,
                },
            );
        }

        let title = if hints.title.is_empty() {
            application.identity.name.clone()
        } else {
            hints.title.clone()
        };
        let process_id = process.as_ref().map(|(record, _)| record.pid);
        let mut window = self.stages.window.resolve(
            window_id.clone(),
            application.id.clone(),
            title,
            &hints,
            process_id,
        );
        window.raw_app_name = Some(raw_app_id.to_string());
        window.resolution_trace.merge(&application.resolution_trace);
        window.resolution_confidence = window.resolution_trace.confidence.max(app_confidence);
        window.resolution_trace.confidence = window.resolution_confidence;

        if let Some(stored) = self.windows.get_mut(&window_id) {
            stored.dirty = false;
        }

        ResolutionAction::Upsert {
            window,
            application,
            application_confidence: app_confidence,
            process,
        }
    }
}
