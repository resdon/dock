# WAYDOCK — Independent Code Audit Instructions

You are the independent senior Rust, Wayland, Vulkan, wgpu and Slint code auditor.

Your task is to inspect the complete repository after the rendering backend migration.

**Do not assume the implementation is correct merely because it compiles.**

---

## 1. Architecture Audit

Verify:

- [ ] wgpu is the only rendering backend
- [ ] Vulkan is the selected backend
- [ ] FemtoVG rendering has been removed
- [ ] CPU/software rendering has been removed (or documented as necessary fallback)
- [ ] Slint remains functional
- [ ] No duplicate rendering systems exist
- [ ] No unnecessary compatibility layers remain
- [ ] wgpu-api crate has actual wgpu dependency (not just types)
- [ ] DMA-BUF presentation is wired for GPU surfaces

## 2. CPU Audit

Identify:

- [ ] Continuous idle rendering
- [ ] Busy loops
- [ ] Excessive event dispatch
- [ ] Unnecessary threads
- [ ] Redundant scene updates
- [ ] Unnecessary layout recalculation
- [ ] Per-frame allocations
- [ ] Excessive synchronization

**Commands to run:**
```bash
# Check for idle CPU
cargo run &
sleep 60
ps -o %cpu,rss -p $(pgrep dock)

# Profile with perf
sudo perf record -g -p $(pgrep dock) -- sleep 30
sudo perf report --sort symbol
```

## 3. Memory Audit

Identify:

- [ ] Memory leaks
- [ ] Unbounded caches
- [ ] Duplicate textures
- [ ] Duplicate scene storage
- [ ] Excessive allocations
- [ ] Unnecessary image copies
- [ ] Unreleased resources

**Commands to run:**
```bash
# Heaptrack
heaptrack -- cargo run &
sleep 30
heaptrack_print /tmp/heaptrack.result

# Check for leaks
valgrind --leak-check=full cargo run 2>&1 | grep -i "definitely lost"
```

## 4. GPU Audit

Identify:

- [ ] Invalid GPU resource usage
- [ ] Synchronization errors
- [ ] Excessive render passes
- [ ] Redundant texture uploads
- [ ] Unnecessary intermediate textures
- [ ] Excessive VRAM usage
- [ ] Unnecessary GPU readback
- [ ] Pipeline recreation
- [ ] Incorrect alpha compositing

**Commands to run:**
```bash
# Enable validation
WGPU_VALIDATION=1 VK_INSTANCE_LAYERS=VK_LAYER_KHRONOS_validation cargo run 2>&1 | grep -i "error\|warning"

# Check with RenderDoc
RENDERDOC_CAPTURE=1 cargo run
```

## 5. Wayland Audit

Inspect:

- [ ] Raw handle safety
- [ ] Surface lifecycle
- [ ] Layer-shell configuration
- [ ] Output handling
- [ ] Frame scheduling
- [ ] Presentation behavior
- [ ] Surface destruction order
- [ ] Resize and recovery behavior

## 6. Slint Audit

Verify:

- [ ] Widget compatibility
- [ ] Layout correctness
- [ ] Text rendering
- [ ] Image rendering
- [ ] Event propagation
- [ ] Focus management
- [ ] Property synchronization
- [ ] Animation behavior
- [ ] Accessibility

**Do not invent unsupported Slint APIs.**

## 7. Rust Safety Audit

Inspect:

- [ ] Unsafe blocks — are they justified?
- [ ] Lifetime assumptions — are they correct?
- [ ] Thread safety — any data races?
- [ ] Resource ownership — clear ownership chains?
- [ ] Error handling — proper Result propagation?
- [ ] Panic conditions — any unexpected panics?
- [ ] Dependency vulnerabilities — run `cargo audit`

## 8. Performance Audit

Measure and report:

| Metric | Target | Measured |
|--------|--------|----------|
| Idle CPU usage | Near 0% | |
| Active CPU usage | <20% | |
| Idle RAM | Bounded | |
| Peak RAM | Measured under stress | |
| VRAM usage | Bounded | |
| Frame times | ≤16.67ms | |
| Input responsiveness | <50ms | |
| GPU execution time | Minimal | |

Compare results against the previous implementation where possible.

## 9. Reporting

Classify every finding:

- **CRITICAL**: correctness, security, or data corruption
- **HIGH**: crashes, leaks, broken rendering, or serious performance problems
- **MEDIUM**: inefficiency or incomplete compatibility
- **LOW**: maintainability and minor optimization

For every finding, provide:

1. **File and line** — Exact location
2. **Explanation** — What is wrong
3. **Reproduction steps** — How to reproduce
4. **Suggested fix** — How to fix it
5. **Verification method** — How to verify the fix

**Do not modify code during the initial audit.**

First produce an independent audit report. Wait for approval before applying fixes.

## Quick Reference Commands

```bash
# Static analysis
cargo fmt --all -- --check
cargo check --workspace --all-targets
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace

# Dependency checks
cargo audit
cargo machete
cargo tree -d

# Performance profiling
sudo perf record -g -p $(pgrep dock) -- sleep 30
heaptrack -- cargo run
MANGOHUD=1 cargo run

# GPU validation
WGPU_VALIDATION=1 cargo run 2>&1 | grep -i "error"
```
