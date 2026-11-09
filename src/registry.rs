//! System registry.
//!
//! Tracks available system resources and capabilities.

use std::collections::HashMap;
use std::path::PathBuf;

use wayland_client::backend::ObjectId;
use wayland_client::protocol::wl_output::WlOutput;
use wayland_client::Proxy;

use crate::models::system::{
    AppId, ApplicationPriority, ApplicationRecord, BinaryContext, OutputGeometry, OutputId,
    OutputRecord, OutputStatus, ProcessId, ProcessRecord, SystemTelemetry, WindowId,
    WindowPriority, WindowRecord, WindowStatus,
};
use crate::models::MediaWindowLink;
use crate::resolvers::ResolutionAction;

fn push_unique<T: PartialEq>(items: &mut Vec<T>, item: T) {
    if !items.iter().any(|existing| existing == &item) {
        items.push(item);
    }
}

fn retain_value<T: PartialEq>(items: &mut Vec<T>, item: &T) {
    items.retain(|existing| existing != item);
}

/// Central in-memory system registry used to keep application, process,
/// window, and output relationships stable and cheaply queryable.
#[derive(Default, Debug, Clone)]
pub struct SystemRegistry {
    pub windows: HashMap<WindowId, WindowRecord>,
    pub applications: HashMap<AppId, ApplicationRecord>,
    pub processes: HashMap<ProcessId, ProcessRecord>,
    pub outputs: HashMap<OutputId, OutputRecord>,

    pub pid_to_app: HashMap<ProcessId, AppId>,

    pub windows_by_app: HashMap<AppId, Vec<WindowId>>,
    pub windows_by_process: HashMap<ProcessId, Vec<WindowId>>,
    pub windows_by_output: HashMap<OutputId, Vec<WindowId>>,
    pub windows_by_status: HashMap<WindowStatus, Vec<WindowId>>,
    pub windows_by_priority: HashMap<WindowPriority, Vec<WindowId>>,

    pub processes_by_app: HashMap<AppId, Vec<ProcessId>>,
    pub processes_by_binary_path: HashMap<PathBuf, Vec<ProcessId>>,
    pub processes_by_binary_name: HashMap<String, Vec<ProcessId>>,

    pub telemetry: SystemTelemetry,
}

impl SystemRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn ensure_application(&mut self, app_id: impl Into<AppId>) -> &mut ApplicationRecord {
        let app_id = app_id.into();
        self.applications
            .entry(app_id.clone())
            .or_insert_with(|| ApplicationRecord::new(app_id))
    }

    pub fn ensure_output(&mut self, output_id: impl Into<OutputId>) -> &mut OutputRecord {
        let output_id = output_id.into();
        self.outputs
            .entry(output_id.clone())
            .or_insert_with(|| OutputRecord::new(output_id))
    }

    pub fn ensure_process(&mut self, pid: ProcessId) -> &mut ProcessRecord {
        self.processes
            .entry(pid)
            .or_insert_with(|| ProcessRecord::new(pid))
    }

    fn detach_process_indexes(&mut self, process: &ProcessRecord) {
        self.pid_to_app.remove(&process.pid);
        if let Some(app_id) = &process.app_id {
            if let Some(app) = self.applications.get_mut(app_id) {
                app.processes.remove(&process.pid);
                if app.windows.is_empty() {
                    app.status.running = false;
                }
            }
            if let Some(list) = self.processes_by_app.get_mut(app_id) {
                retain_value(list, &process.pid);
            }
        }

        if let Some(path) = &process.binary.path {
            if let Some(list) = self.processes_by_binary_path.get_mut(path) {
                retain_value(list, &process.pid);
            }
        }

        if let Some(name) = &process.binary.executable {
            if let Some(list) = self.processes_by_binary_name.get_mut(name) {
                retain_value(list, &process.pid);
            }
        }
    }

    fn attach_process_indexes(&mut self, process: &ProcessRecord) {
        if let Some(app_id) = &process.app_id {
            self.ensure_application(app_id.clone())
                .processes
                .insert(process.pid);
            push_unique(
                self.processes_by_app.entry(app_id.clone()).or_default(),
                process.pid,
            );
        }

        if let Some(path) = &process.binary.path {
            push_unique(
                self.processes_by_binary_path
                    .entry(path.clone())
                    .or_default(),
                process.pid,
            );
        }

        if let Some(name) = &process.binary.executable {
            push_unique(
                self.processes_by_binary_name
                    .entry(name.clone())
                    .or_default(),
                process.pid,
            );
        }
    }

    pub fn register_application(&mut self, mut app: ApplicationRecord) {
        let app_id = app.id.clone();
        if let Some(existing) = self.applications.remove(&app_id) {
            // Relationship indexes are authoritative; preserve them while replacing
            // resolved metadata/status from the new snapshot.
            app.processes.extend(existing.processes);
            for window_id in existing.windows {
                push_unique(&mut app.windows, window_id);
            }
            if matches!(app.priority, ApplicationPriority::Normal) {
                app.priority = existing.priority;
            }
        }
        self.applications.insert(app_id.clone(), app);
        self.refresh_application_status(&app_id);
    }

    pub fn register_output(&mut self, mut output: OutputRecord) {
        let output_id = output.id.clone();
        if let Some(existing) = self.outputs.remove(&output_id) {
            for window_id in existing.windows {
                push_unique(&mut output.windows, window_id);
            }
        }
        self.outputs.insert(output_id, output);
    }

    pub fn register_process(&mut self, mut process: ProcessRecord, app_id: Option<AppId>) {
        let pid = process.pid;

        if let Some(existing) = self.processes.remove(&pid) {
            self.detach_process_indexes(&existing);
        }

        process.app_id = app_id.clone();
        process.status.running = true;
        self.processes.insert(pid, process);

        if let Some(app_id) = app_id {
            self.pid_to_app.insert(pid, app_id.clone());
            let app = self.ensure_application(app_id);
            app.status.running = true;
            app.processes.insert(pid);
        }

        if let Some(process) = self.processes.get(&pid).cloned() {
            self.attach_process_indexes(&process);
        }
    }

    pub fn remove_process(&mut self, pid: ProcessId) {
        if let Some(process) = self.processes.remove(&pid) {
            self.detach_process_indexes(&process);
            if let Some(app_id) = process.app_id {
                self.refresh_application_status(&app_id);
            }
        }
    }

    fn detach_window_indexes(&mut self, window: &WindowRecord) {
        if let Some(app) = self.applications.get_mut(&window.app_id) {
            retain_value(&mut app.windows, &window.id);
        }

        if let Some(list) = self.windows_by_app.get_mut(&window.app_id) {
            retain_value(list, &window.id);
        }
        if let Some(pid) = window.process_id {
            if let Some(list) = self.windows_by_process.get_mut(&pid) {
                retain_value(list, &window.id);
            }
        }
        if let Some(list) = self.windows_by_status.get_mut(&window.status) {
            retain_value(list, &window.id);
        }
        if let Some(list) = self.windows_by_priority.get_mut(&window.priority) {
            retain_value(list, &window.id);
        }
        for output_id in &window.outputs {
            if let Some(list) = self.windows_by_output.get_mut(output_id) {
                retain_value(list, &window.id);
            }
            if let Some(output) = self.outputs.get_mut(output_id) {
                retain_value(&mut output.windows, &window.id);
            }
        }

        self.refresh_application_status(&window.app_id);
    }

    fn attach_window_indexes(&mut self, window: &WindowRecord) {
        let app = self.ensure_application(window.app_id.clone());
        app.status.running = true;
        push_unique(&mut app.windows, window.id.clone());

        push_unique(
            self.windows_by_app
                .entry(window.app_id.clone())
                .or_default(),
            window.id.clone(),
        );
        if let Some(pid) = window.process_id {
            push_unique(
                self.windows_by_process.entry(pid).or_default(),
                window.id.clone(),
            );
        }
        push_unique(
            self.windows_by_status.entry(window.status).or_default(),
            window.id.clone(),
        );
        push_unique(
            self.windows_by_priority.entry(window.priority).or_default(),
            window.id.clone(),
        );
        for output_id in &window.outputs {
            push_unique(
                self.windows_by_output.entry(output_id.clone()).or_default(),
                window.id.clone(),
            );
            let output = self.ensure_output(output_id.clone());
            push_unique(&mut output.windows, window.id.clone());
        }

        self.refresh_application_status(&window.app_id);
    }

    pub fn register_window(&mut self, window: WindowRecord) {
        let id = window.id.clone();

        if let Some(existing) = self.windows.remove(&id) {
            self.detach_window_indexes(&existing);
        }

        self.windows.insert(id.clone(), window);

        if let Some(window) = self.windows.get(&id).cloned() {
            self.attach_window_indexes(&window);
        }
    }

    pub fn remove_window(&mut self, id: ObjectId) {
        let window_id = WindowId(id);
        if let Some(window) = self.windows.remove(&window_id) {
            self.detach_window_indexes(&window);
        }
    }

    pub fn unregister_window(&mut self, id: ObjectId) {
        self.remove_window(id);
    }

    pub fn sync_output_from_wayland(&mut self, output: &WlOutput, name: Option<String>) {
        let output_id = OutputId(output.id());
        let entry = self.ensure_output(output_id);
        if let Some(name) = name {
            entry.name = Some(name);
        }
    }

    pub fn attach_window_to_output(&mut self, id: ObjectId, output: &WlOutput) {
        let window_id = WindowId(id);
        let output_id = OutputId(output.id());
        self.ensure_output(output_id.clone());

        if let Some(window) = self.windows.get_mut(&window_id) {
            if !window.outputs.iter().any(|existing| existing == &output_id) {
                window.outputs.push(output_id.clone());
            }
        }

        push_unique(
            self.windows_by_output.entry(output_id.clone()).or_default(),
            window_id.clone(),
        );

        if let Some(output_record) = self.outputs.get_mut(&output_id) {
            push_unique(&mut output_record.windows, window_id);
        }
    }

    pub fn detach_window_from_output(&mut self, id: ObjectId, output: &WlOutput) {
        let window_id = WindowId(id);
        let output_id = OutputId(output.id());
        if let Some(window) = self.windows.get_mut(&window_id) {
            window.outputs.retain(|existing| existing != &output_id);
        }
        if let Some(list) = self.windows_by_output.get_mut(&output_id) {
            retain_value(list, &window_id);
        }
        if let Some(output_record) = self.outputs.get_mut(&output_id) {
            retain_value(&mut output_record.windows, &window_id);
        }
    }

    pub fn application_for_window(&self, id: ObjectId) -> Option<&ApplicationRecord> {
        let window_id = WindowId(id);
        self.windows
            .get(&window_id)
            .and_then(|window| self.applications.get(&window.app_id))
    }

    pub fn window_record(&self, id: ObjectId) -> Option<&WindowRecord> {
        self.windows.get(&WindowId(id))
    }

    pub fn window_record_mut(&mut self, id: ObjectId) -> Option<&mut WindowRecord> {
        self.windows.get_mut(&WindowId(id))
    }

    pub fn process_record(&self, pid: ProcessId) -> Option<&ProcessRecord> {
        self.processes.get(&pid)
    }

    pub fn application_record(&self, app_id: &AppId) -> Option<&ApplicationRecord> {
        self.applications.get(app_id)
    }

    pub fn output_record(&self, output_id: &OutputId) -> Option<&OutputRecord> {
        self.outputs.get(output_id)
    }

    pub fn processes_for_app(&self, app_id: &str) -> Vec<&ProcessRecord> {
        let app_id = AppId::from(app_id);
        self.processes_by_app
            .get(&app_id)
            .into_iter()
            .flat_map(|process_ids| process_ids.iter())
            .filter_map(|pid| self.processes.get(pid))
            .collect()
    }

    pub fn windows_for_process(&self, pid: ProcessId) -> Vec<&WindowRecord> {
        self.windows_by_process
            .get(&pid)
            .into_iter()
            .flat_map(|window_ids| window_ids.iter())
            .filter_map(|window_id| self.windows.get(window_id))
            .collect()
    }

    pub fn windows_for_app(&self, app_id: &str) -> Vec<&WindowRecord> {
        let app_id = AppId::from(app_id);
        self.windows_by_app
            .get(&app_id)
            .into_iter()
            .flat_map(|window_ids| window_ids.iter())
            .filter_map(|window_id| self.windows.get(window_id))
            .collect()
    }

    pub fn windows_for_app_ordered(&self, app_id: &str) -> Vec<&WindowRecord> {
        let mut windows = self.windows_for_app(app_id);
        windows.sort_by(|a, b| {
            a.priority
                .cmp(&b.priority)
                .then_with(|| a.status.cmp(&b.status))
                .then_with(|| a.order_index.cmp(&b.order_index))
                .then_with(|| a.title.cmp(&b.title))
        });
        windows
    }

    pub fn windows_for_output(&self, output: &OutputId) -> Vec<&WindowRecord> {
        self.windows_by_output
            .get(output)
            .into_iter()
            .flat_map(|window_ids| window_ids.iter())
            .filter_map(|window_id| self.windows.get(window_id))
            .collect()
    }

    pub fn windows_for_status(&self, status: WindowStatus) -> Vec<&WindowRecord> {
        self.windows_by_status
            .get(&status)
            .into_iter()
            .flat_map(|window_ids| window_ids.iter())
            .filter_map(|window_id| self.windows.get(window_id))
            .collect()
    }

    pub fn windows_for_priority(&self, priority: WindowPriority) -> Vec<&WindowRecord> {
        self.windows_by_priority
            .get(&priority)
            .into_iter()
            .flat_map(|window_ids| window_ids.iter())
            .filter_map(|window_id| self.windows.get(window_id))
            .collect()
    }

    pub fn output_records_for_window(&self, id: ObjectId) -> Vec<&OutputRecord> {
        let window_id = WindowId(id);
        self.windows
            .get(&window_id)
            .map(|window| {
                window
                    .outputs
                    .iter()
                    .filter_map(|output_id| self.outputs.get(output_id))
                    .collect()
            })
            .unwrap_or_default()
    }

    pub fn app_id_for_pid(&self, pid: u32) -> Option<&AppId> {
        self.pid_to_app.get(&ProcessId(pid))
    }

    /// Applies a resolver result atomically at the registry boundary. Resolvers
    /// never mutate the registry directly, which keeps discovery testable and
    /// prevents partially-updated relationship indexes.
    pub fn apply_resolution(&mut self, action: ResolutionAction) -> bool {
        match action {
            ResolutionAction::None => false,
            ResolutionAction::Remove { window_id } => {
                self.remove_window(window_id.0);
                true
            }
            ResolutionAction::Upsert {
                window,
                application,
                application_confidence,
                process,
            } => {
                let app_id = application.id.clone();
                let mut application = application;
                application.resolution_confidence = application
                    .resolution_trace
                    .confidence
                    .max(application_confidence);
                application.resolution_trace.confidence = application.resolution_confidence;
                self.register_application(application);
                if let Some((mut process, confidence)) = process {
                    process.resolution_confidence = process.resolution_confidence.max(confidence);
                    process.resolution_trace.confidence =
                        process.resolution_trace.confidence.max(confidence);
                    self.register_process(process, Some(app_id.clone()));
                }
                let mut window = window;
                window.resolution_trace.confidence = window
                    .resolution_trace
                    .confidence
                    .max(application_confidence);
                window.resolution_confidence = window.resolution_trace.confidence;
                self.register_window(window);
                self.refresh_application_status(&app_id);
                true
            }
        }
    }

    pub fn remove_output(&mut self, output_id: &crate::models::system::OutputId) {
        let mut affected: Vec<WindowId> =
            self.windows_by_output.remove(output_id).unwrap_or_default();

        if let Some(output) = self.outputs.remove(output_id) {
            for window_id in output.windows {
                push_unique(&mut affected, window_id);
            }
        }

        // The window's output list is the relationship source of truth. Repair
        // every affected window even if the output-side index was incomplete.
        for window_id in affected {
            if let Some(window) = self.windows.get_mut(&window_id) {
                window.outputs.retain(|id| id != output_id);
            }
        }
    }

    pub fn set_application_priority(
        &mut self,
        app_id: impl Into<AppId>,
        priority: ApplicationPriority,
    ) {
        let app_id = app_id.into();
        self.ensure_application(app_id.clone()).priority = priority;
    }

    pub fn set_process_binary(&mut self, pid: ProcessId, binary: BinaryContext) {
        if let Some(existing) = self.processes.remove(&pid) {
            self.detach_process_indexes(&existing);
        }

        let snapshot = {
            let process = self.ensure_process(pid);
            process.binary = binary;
            process.status.running = true;
            process.clone()
        };
        self.attach_process_indexes(&snapshot);
    }

    pub fn set_window_priority(&mut self, id: ObjectId, priority: WindowPriority) {
        let window_id = WindowId(id);
        if let Some(mut window) = self.windows.remove(&window_id) {
            self.detach_window_indexes(&window);
            window.priority = priority;
            self.register_window(window);
        }
    }

    pub fn touch_window_status(&mut self, id: ObjectId, status: WindowStatus) {
        let window_id = WindowId(id);
        if let Some(mut window) = self.windows.remove(&window_id) {
            self.detach_window_indexes(&window);
            window.status = status;
            window.is_activated = matches!(status, WindowStatus::Active);
            window.is_minimized = matches!(status, WindowStatus::Minimized);
            window.is_foreground = matches!(status, WindowStatus::Active);
            window.is_background = matches!(status, WindowStatus::Background);
            window.is_locked = matches!(status, WindowStatus::Hidden);
            self.register_window(window);
        }
    }

    pub fn update_output_geometry(&mut self, output_id: &OutputId, geometry: OutputGeometry) {
        self.ensure_output(output_id.clone()).geometry = geometry;
    }

    pub fn update_output_status(&mut self, output_id: &OutputId, status: OutputStatus) {
        self.ensure_output(output_id.clone()).status = status;
    }

    pub fn media_windows(&self) -> Vec<&WindowRecord> {
        let mut windows: Vec<&WindowRecord> = self
            .windows
            .values()
            .filter(|window| window.media.is_some())
            .collect();
        windows.sort_by(|a, b| {
            b.is_activated
                .cmp(&a.is_activated)
                .then_with(|| b.priority.cmp(&a.priority))
                .then_with(|| a.order_index.cmp(&b.order_index))
                .then_with(|| a.title.cmp(&b.title))
        });
        windows
    }

    pub fn media_windows_for_app(&self, app_id: &str) -> Vec<&WindowRecord> {
        let app_id = AppId::from(app_id);
        self.windows
            .values()
            .filter(|window| window.app_id == app_id && window.media.is_some())
            .collect()
    }

    pub fn update_window_media(&mut self, id: ObjectId, media: Option<MediaWindowLink>) {
        if let Some(window) = self.windows.get_mut(&WindowId(id)) {
            window.media = media;
        }
    }

    pub fn set_telemetry(&mut self, telemetry: SystemTelemetry) {
        self.telemetry = telemetry;
    }

    pub fn refresh_application_status(&mut self, app_id: &AppId) {
        let (window_ids, process_count) = if let Some(app) = self.applications.get(app_id) {
            (app.windows.clone(), app.processes.len())
        } else {
            return;
        };

        let windows: Vec<WindowRecord> = window_ids
            .iter()
            .filter_map(|id| self.windows.get(id).cloned())
            .collect();

        let running = !windows.is_empty() || process_count > 0;
        let foreground = windows.iter().any(|w| w.is_activated);
        let background = windows.iter().any(|w| !w.is_activated);
        let hidden =
            !windows.is_empty() && windows.iter().all(|w| w.status == WindowStatus::Hidden);
        let locked = windows.iter().any(|w| w.is_locked);
        let attention = windows
            .iter()
            .any(|w| matches!(w.status, WindowStatus::Active));

        if let Some(app) = self.applications.get_mut(app_id) {
            app.status.running = running;
            app.status.foreground = foreground;
            app.status.background = background;
            app.status.hidden = hidden;
            app.status.locked = locked;
            app.status.attention = attention;
        }
    }

    /// Debug/test invariant check for all registry relationship indexes.
    /// Keeping this here makes index corruption detectable without exposing
    /// callers to the internal maintenance details.
    pub fn validate_indexes(&self) -> Result<(), String> {
        for (id, window) in &self.windows {
            if !self
                .windows_by_app
                .get(&window.app_id)
                .map_or(false, |v| v.contains(id))
            {
                return Err(format!("window {:?} missing windows_by_app index", id));
            }
            if let Some(pid) = window.process_id {
                if !self
                    .windows_by_process
                    .get(&pid)
                    .map_or(false, |v| v.contains(id))
                {
                    return Err(format!("window {:?} missing windows_by_process index", id));
                }
            }
            if !self
                .windows_by_status
                .get(&window.status)
                .map_or(false, |v| v.contains(id))
            {
                return Err(format!("window {:?} missing windows_by_status index", id));
            }
            if !self
                .windows_by_priority
                .get(&window.priority)
                .map_or(false, |v| v.contains(id))
            {
                return Err(format!("window {:?} missing windows_by_priority index", id));
            }
            for output in &window.outputs {
                if !self
                    .windows_by_output
                    .get(output)
                    .map_or(false, |v| v.contains(id))
                {
                    return Err(format!("window {:?} missing output index", id));
                }
            }
        }

        for (pid, process) in &self.processes {
            match &process.app_id {
                Some(app_id) => {
                    if self.pid_to_app.get(pid) != Some(app_id) {
                        return Err(format!("pid {:?} has stale pid_to_app mapping", pid));
                    }
                    if !self
                        .processes_by_app
                        .get(app_id)
                        .map_or(false, |v| v.contains(pid))
                    {
                        return Err(format!("process {:?} missing processes_by_app index", pid));
                    }
                }
                None if self.pid_to_app.contains_key(pid) => {
                    return Err(format!("pid {:?} has mapping without app", pid));
                }
                None => {}
            }
        }

        Ok(())
    }
}
