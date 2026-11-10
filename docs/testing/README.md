# WAYDOCK — Testing and Quality Control

This directory contains the testing infrastructure and documentation for the Waydock project.

## 8-Layer Quality Control Pipeline

| Layer | Description | Files |
|-------|-------------|-------|
| 0. **Testing Protocol** | **Mandatory 22-phase AI agent testing & verification protocol** | `TESTING_PROTOCOL.md` |
| 1. Static Analysis | Rust compilation, Clippy, formatting, dependency checks | `scripts/check.sh` |
| 2. Profiling | CPU and RAM profiling with perf, heaptrack, Massif, Tracy | `profiling.md` |
| 3. GPU Validation | wgpu error scopes, Vulkan validation, RenderDoc, MangoHud | `gpu-validation.md` |
| 4. Compatibility Tests | Wayland and Slint compatibility test framework | `../tests/compatibility.rs` |
| 5. Visual Regression | Screenshot comparison with tolerance-based testing | `visual-regression.md` |
| 6. Fuzzing | cargo-fuzz targets for config, scene, geometry, animation | `fuzzing.md` |
| 7. AI Code Audit | Independent code audit instructions for AI agents | `AI_AUDIT.md` |
| 8. CI Pipeline | GitHub Actions automated checks on push/PR | `../.github/workflows/ci.yml` |

## Testing Protocol

See [TESTING_PROTOCOL.md](TESTING_PROTOCOL.md) for the **mandatory 22-phase testing protocol** that every AI agent must follow. This includes:

- Repository inspection and static analysis
- Dependency and security checks
- Automated tests with required coverage
- Shader validation and GPU/Vulkan testing
- Wayland and Slint compatibility tests
- Visual regression testing
- CPU, RAM, and VRAM profiling
- Responsiveness and frame timing
- Stress and stability tests
- Fuzzing and safety checks
- Performance regression comparison
- Git safety procedures
- Mandatory testing report template
- Three testing gates (Build correctness, Runtime correctness, Performance acceptance)

## Quick Start

```bash
# Run all static analysis checks
./scripts/check.sh

# Run with fast mode (skip slow checks)
./scripts/check.sh --fast

# Run CI-mode (fail on first error)
./scripts/check.sh --ci

# Run physics tests
cd wgpu-api && cargo test physics

# Run all workspace tests
cargo test --workspace
```

## Performance Profiling

See [profiling.md](profiling.md) for detailed instructions on:

- CPU profiling with `perf` and Tracy
- RAM profiling with `heaptrack` and Valgrind Massif
- Performance test scenarios (idle, hover, animation, image load, orientation switch)
- Performance report template

## GPU Validation

See [gpu-validation.md](gpu-validation.md) for:

- wgpu error scope usage
- Vulkan validation layer setup
- RenderDoc frame capture
- MangoHud FPS monitoring
- GPU resource lifetime tests

## Visual Regression Testing

See [visual-regression.md](visual-regression.md) for:

- Reference screenshot creation
- ImageMagick comparison with tolerance
- Custom Rust comparison code
- Tolerance settings per scenario

## Fuzzing

See [fuzzing.md](fuzzing.md) for:

- cargo-fuzz setup
- Fuzz targets (config, scene, geometry, animation, texture, input)
- Stress tests (many icons, rapid pointer, popup open/close)
- Miri for memory safety checks

## AI Code Audit

See [AI_AUDIT.md](AI_AUDIT.md) for the independent code audit template covering:

- Architecture audit
- CPU and memory audit
- GPU audit
- Wayland and Slint audit
- Rust safety audit
- Performance audit with measurement template

## CI Pipeline

The GitHub Actions workflow (`.github/workflows/ci.yml`) runs on every push and PR:

1. **Static Analysis** — fmt, check, clippy, tests, dependency duplicates
2. **Dependency Security** — cargo audit
3. **Physics Tests** — wgpu-api physics test suite
4. **Documentation** — cargo doc build

**Note:** GPU integration tests require a Vulkan-capable runner with Wayland, which is not available on standard GitHub Actions runners. These must be run locally or on a self-hosted runner.

## Testing Stack Summary

| Category | Tool |
|----------|------|
| Compilation | Cargo |
| Static analysis | Clippy |
| Formatting | rustfmt |
| Dependency security | cargo-audit |
| Dependency cleanup | cargo-machete |
| Unit tests | cargo-nextest |
| CPU profiling | perf |
| RAM profiling | heaptrack |
| GPU profiling | Tracy |
| GPU debugging | RenderDoc |
| Vulkan validation | Validation Layers |
| FPS monitoring | MangoHud |
| Fuzzing | cargo-fuzz |
| Unsafe-code checks | Miri |
| Visual regression | Screenshot comparison |
| AI review | Independent coding agent |
