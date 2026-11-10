# Build, Test, and Regression Procedures

## Build Commands

### Check Compilation

```bash
# Quick check (no code generation)
cargo check

# Check all targets including tests and benchmarks
cargo check --all-targets

# Check specific crate
cargo check -p wgpu-api
```

### Full Build

```bash
# Debug build
cargo build

# Release build
cargo build --release

# Build specific crate
cargo build -p wgpu-api
```

## Test Commands

### Physics Tests

```bash
# Run all physics tests
cd wgpu-api && cargo test physics

# Run specific physics module
cd wgpu-api && cargo test physics::springs
cd wgpu-api && cargo test physics::collision
cd wgpu-api && cargo test physics::fields
cd wgpu-api && cargo test physics::constraints
cd wgpu-api && cargo test physics::rapier
cd wgpu-api && cargo test physics::timestep
cd wgpu-api && cargo test physics::commands
cd wgpu-api && cargo test physics::interaction
```

### All Tests

```bash
# Run all tests in wgpu-api
cd wgpu-api && cargo test

# Run all tests in root crate
cargo test
```

### Test Filtering

```bash
# Run tests matching a pattern
cargo test physics::springs::test_spring_force

# Run only doc tests
cargo test --doc

# Run tests in a specific file
cargo test --test integration_tests
```

## Benchmark Commands

### Criterion Benchmarks

```bash
# Run all benchmarks
cargo bench

# Run specific benchmark
cargo bench physics_step

# Generate HTML report
cargo bench -- --output-format bencher | cargo bench run
```

### Profiling

```bash
# Profile with perf
perf record -g cargo run --release
perf report

# Profile with flamegraph
cargo install flamegraph
flamegraph cargo run --release

# Profile with samply (sampling profiler)
cargo install samply
samply run -- cargo run --release
```

## Verification Matrix

| Change Type | Verification Method |
|-------------|---------------------|
| Physics changes | `cargo test physics` in `wgpu-api/` |
| Rendering changes | Visual inspection + `cargo check` |
| Wayland changes | Run under labwc, check window behavior |
| Video changes | Run with PipeWire stream active |
| General compilation | `cargo check` at project root |
| Full test suite | `cargo test` in `wgpu-api/` |
| Performance changes | `cargo bench` + visual FPS check |
| Dependency upgrades | `cargo check` + `cargo test` |

## Regression Testing

### Pre-Existing Test Failures

The following 5 tests are known to fail due to incorrect assertions, not implementation bugs:

1. `physics::collision::tests::test_resolve_collision` — edge case assertion
2. `physics::fields::tests::test_falloff_exponent` — edge case assertion
3. `physics::solver::tests::test_solver_map` — edge case assertion
4. `physics::spatial_index::tests::test_insert_and_query_rect` — edge case assertion
5. `physics::springs::tests::test_fixed_point_spring` — edge case assertion

**Expected test count**: 334 passed, 5 failed.

If the number of passing tests drops below 334, investigate the regression.

### Visual Regression Checklist

After rendering changes:
- [ ] Dock icons render correctly
- [ ] Physics animations are smooth
- [ ] Video capture displays correctly
- [ ] Popups and system bar render
- [ ] No GPU errors in logs
- [ ] FPS remains above 60

### Integration Regression Checklist

After Wayland/PipeWire changes:
- [ ] Dock window appears on screen
- [ ] Input events are processed
- [ ] Screen capture starts and stops
- [ ] D-Bus tray icons appear
- [ ] Bluetooth monitoring works
- [ ] No compositor crashes

## CI/CD Integration

### Recommended GitHub Actions Workflow

```yaml
name: CI
on: [push, pull_request]
jobs:
  check:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - uses: dtolnay/rust-toolchain@stable
      - run: cargo check --all-targets
      - run: cd wgpu-api && cargo test physics
```

## Code Quality Checks

### Clippy

```bash
# Run Clippy lints
cargo clippy -- -D warnings

# Run Clippy for specific crate
cargo clippy -p wgpu-api -- -D warnings
```

### Formatting

```bash
# Check formatting
cargo fmt --check

# Apply formatting
cargo fmt
```

### Documentation

```bash
# Generate documentation
cargo doc --no-deps

# Check documentation links
cargo doc --no-deps --document-private-items
```

## Testing Invariants

1. **All new code** must have corresponding tests.
2. **Physics changes** must pass `cargo test physics`.
3. **Rendering changes** must pass visual inspection.
4. **Dependency upgrades** must pass full test suite.
5. **Performance changes** must not regress FPS below 60.
6. **No test failures** should be introduced without explicit documentation.

## Reference

- **Performance targets**: `docs/architecture/performance.md`
- **Known issues**: `docs/architecture/known_issues.md`
- **Coding rules**: `docs/architecture/coding_rules.md`
- **Rust engineering**: `docs/reference/rust-engineering.md`
