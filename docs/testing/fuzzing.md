# WAYDOCK — Fuzzing Guide

This document describes how to set up and run fuzzing tests for the Waydock application.

## Setup

Install cargo-fuzz:

```bash
cargo install cargo-fuzz
```

Initialize fuzzing in the wgpu-api crate:

```bash
cd wgpu-api
cargo +nightly fuzz init
```

## Fuzz Targets

Focus fuzzing on these areas:

### 1. Configuration Parsing

Fuzz the configuration file parser to find crashes or unexpected behavior.

```bash
cargo +nightly fuzz add config_parsing
```

```rust
// fuzz/fuzz_targets/config_parsing.rs
#![no_main]

libfuzzer_sys::fuzz_target!(|data: &[u8]| {
    if let Ok(text) = core::str::from_utf8(data) {
        // Try to parse as configuration
        let _ = serde_json::from_str::<serde_json::Value>(text);
    }
});
```

### 2. Scene Updates

Fuzz scene graph operations to find crashes in rendering.

```bash
cargo +nightly fuzz add scene_update
```

### 3. Geometry Calculations

Fuzz geometry operations (rectangles, circles, paths).

```bash
cargo +nightly fuzz add geometry
```

### 4. Animation Parameters

Fuzz animation parameters (easing, keyframes, springs).

```bash
cargo +nightly fuzz add animation_params
```

### 5. Texture Metadata

Fuzz texture format and dimension parsing.

```bash
cargo +nightly fuzz add texture_metadata
```

### 6. Input Event Sequences

Fuzz sequences of input events to find state machine bugs.

```bash
cargo +nightly fuzz add input_events
```

## Running Fuzzers

```bash
# Run a fuzzer for 60 seconds
cargo +nightly fuzz run config_parsing -- -max_total_time=60

# Run with multiple jobs
cargo +nightly fuzz run config_parsing -j4

# Resume from corpus
cargo +nightly fuzz run config_parsing

# Minimize corpus
cargo +nightly fuzz cmin config_parsing
```

## Stress Tests

In addition to fuzzing, run these stress tests:

### 1. Hundreds of Icons

```rust
#[test]
fn test_many_icons() {
    // Create a dock with 200 icons
    // Verify rendering doesn't crash
}
```

### 2. Rapid Pointer Movement

Simulate rapid pointer movement over the dock.

### 3. Repeated Popup Open/Close

Open and close popups 1000 times rapidly.

### 4. Rapid Theme Changes

Switch themes 100 times rapidly.

### 5. Repeated Surface Resize

Resize the surface 100 times rapidly.

### 6. Memory Pressure

Allocate and release textures under memory pressure.

## Miri

Run Rust tests with Miri to catch unsafe-code and memory-model problems:

```bash
# Install Miri
rustup +nightly component add miri
cargo +nightly miri setup

# Run tests under Miri
cd wgpu-api
cargo +nightly miri test physics
```

**Note:** Miri won't validate Vulkan or Wayland behavior directly. It checks Rust memory safety.

## Tools Summary

| Tool | Purpose | Command |
|------|---------|---------|
| `cargo-fuzz` | Fuzzing with libFuzzer | `cargo +nightly fuzz run <target>` |
| `Miri` | Memory safety checks | `cargo +nightly miri test` |
| `proptest` | Property-based testing | Built into test suite |
