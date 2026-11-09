//! Process state resolver.

use super::context::{normalize_token, read_proc_environment, ResolverSnapshot};
use crate::models::system::{
    ApplicationRecord, ProcessId, ProcessPriority, ProcessRecord, ResolutionConfidence,
    ResolutionEvidence, ResolutionTrace,
};

#[derive(Debug, Default, Clone, Copy)]
pub struct ProcessResolver;

impl ProcessResolver {
    pub fn find_candidate(
        &self,
        app_id: &str,
        title: &str,
        snapshot: &ResolverSnapshot,
    ) -> Option<(ProcessId, ResolutionConfidence)> {
        self.find_process(app_id, title, snapshot)
    }

    pub fn resolve_candidate(
        &self,
        pid: ProcessId,
        confidence: ResolutionConfidence,
        snapshot: &ResolverSnapshot,
        application: &ApplicationRecord,
    ) -> Option<(ProcessRecord, ResolutionConfidence)> {
        let observation = snapshot.processes.iter().find(|p| p.pid == pid)?;

        let mut record = ProcessRecord::new(pid);
        let mut trace = ResolutionTrace::new(confidence, [ResolutionEvidence::ProcessId]);
        record.app_id = Some(application.id.clone());
        record.ppid = observation.ppid;
        record.binary.path = observation.executable.clone();
        record.binary.executable = observation.executable_name();
        record.binary.working_directory = observation.cwd.clone();
        record.binary.command_line = Some(observation.command_line.join(" "));
        record.path = record.binary.path.clone();
        record.working_directory = record.binary.working_directory.clone();
        record.env_vars = read_proc_environment(pid);
        record.platform = application.platform.clone();
        record.status.running = true;
        record.status.responding = true;
        record.priority = ProcessPriority::Normal;
        record.telemetry.cpu.usage_percent = observation.cpu_usage;
        record.telemetry.memory_bytes = observation.memory_bytes;
        if record.binary.path.is_some() {
            trace.add(ResolutionEvidence::ExecutablePath);
        }
        if record.binary.executable.is_some() {
            trace.add(ResolutionEvidence::ExecutableName);
        }
        if !record.env_vars.is_empty() {
            trace.add(ResolutionEvidence::ProcessEnvironment);
        }
        record.resolution_confidence = trace.confidence;
        record.resolution_trace = trace.clone();
        Some((record, trace.confidence))
    }

    pub fn observe_cached(&self, record: &mut ProcessRecord, snapshot: &ResolverSnapshot) -> bool {
        let Some(observation) = snapshot.processes.iter().find(|p| p.pid == record.pid) else {
            record.status.running = false;
            return false;
        };
        record.ppid = observation.ppid;
        record.resolution_trace.add(ResolutionEvidence::ProcessId);
        if observation.executable.is_some() {
            record
                .resolution_trace
                .add(ResolutionEvidence::ExecutablePath);
        }
        record.telemetry.cpu.usage_percent = observation.cpu_usage;
        record.telemetry.memory_bytes = observation.memory_bytes;
        record.status.running = true;
        record.status.responding = true;
        true
    }

    fn find_process(
        &self,
        app_id: &str,
        title: &str,
        snapshot: &ResolverSnapshot,
    ) -> Option<(ProcessId, ResolutionConfidence)> {
        let target = normalize_token(app_id);
        let title = normalize_token(title);
        if target.is_empty() && title.is_empty() {
            return None;
        }

        let mut best: Option<(ProcessId, ResolutionConfidence, i32)> = None;
        for process in &snapshot.processes {
            let name = normalize_token(&process.name);
            let exe = normalize_token(&process.executable_name().unwrap_or_default());
            let candidate = if !target.is_empty() && (name == target || exe == target) {
                Some((ResolutionConfidence::Exact, 100))
            } else if !target.is_empty()
                && (!name.is_empty()
                    && (name == target || name.contains(&target) || target.contains(&name)))
            {
                Some((ResolutionConfidence::Strong, 70))
            } else if !target.is_empty()
                && !exe.is_empty()
                && (exe.contains(&target) || target.contains(&exe))
            {
                Some((ResolutionConfidence::Strong, 65))
            } else if !title.is_empty()
                && !name.is_empty()
                && (title.contains(&name) || name.contains(&title))
            {
                Some((ResolutionConfidence::Probable, 35))
            } else {
                None
            };

            if let Some((confidence, score)) = candidate {
                if best
                    .as_ref()
                    .map(|(_, _, old)| score > *old)
                    .unwrap_or(true)
                {
                    best = Some((process.pid, confidence, score));
                }
            }
        }
        best.map(|(pid, confidence, _)| (pid, confidence))
    }
}
