# Rust Engineering Documentation

This is the most important reference section. These resources are **not optional** if you want an AI agent to make reliable changes to the Dock codebase. They cover the Rust language, tooling, best practices, and release process.

---

## 1. The Rust Book

| Resource | URL |
|----------|-----|
| The Rust Programming Language | https://doc.rust-lang.org/book |
| GitHub Repository | https://github.com/rust-lang/book |
| Table of Contents | https://doc.rust-lang.org/book/table-of-contents.html |

**Relevance to Dock:** The definitive guide to the Rust language. Covers ownership, borrowing, lifetimes, traits, error handling, concurrency, and more. Every AI agent working on Dock should have read this.

**Key Chapters for Dock:**
- **Ch. 4: Ownership** — Understanding memory management without a garbage collector
- **Ch. 10: Generics, Traits, and Lifetimes** — Essential for wgpu-api's generic APIs
- **Ch. 16: Fearless Concurrency** — Thread safety for Wayland, PipeWire, and rendering
- **Ch. 17: Async Rust** — Async/await for D-Bus and PipeWire operations

---

## 2. Rust Standard Library

| Resource | URL |
|----------|-----|
| std Documentation | https://doc.rust-lang.org/std |
| Collections | https://doc.rust-lang.org/std/collections |
| I/O | https://doc.rust-lang.org/std/io |
| Sync Primitives | https://doc.rust-lang.org/std/sync |
| Thread Module | https://doc.rust-lang.org/std/thread |
| Result and Option | https://doc.rust-lang.org/std/result and https://doc.rust-lang.org/std/option |

**Relevance to Dock:** The standard library is the foundation of all Rust code. Dock uses:
- `Vec`, `HashMap`, `BTreeMap` for collections
- `Result`, `Option` for error handling
- `Arc`, `Mutex`, `RwLock` for shared state
- `File`, `Read`, `Write` for file I/O
- `thread`, `channel` for threading

---

## 3. Cargo Book

| Resource | URL |
|----------|-----|
| The Cargo Book | https://doc.rust-lang.org/cargo |
| Cargo Reference | https://doc.rust-lang.org/cargo/reference |
| Cargo Commands | https://doc.rust-lang.org/cargo/commands |
| Cargo.toml Format | https://doc.rust-lang.org/cargo/reference/manifest.html |
| Feature Unification | https://doc.rust-lang.org/cargo/reference/features.html |

**Relevance to Dock:** Cargo is Rust's build system and package manager. Dock uses:
- Workspace configuration (root crate + wgpu-api library)
- Feature flags for optional dependencies
- Build scripts for Slint compilation
- Dependency version management

**Key Concepts:**
- **Workspace**: Multiple crates in one project (dock + wgpu-api)
- **Features**: Optional dependency activation
- **Build scripts**: Pre-build compilation steps (slint-build)
- **Profiles**: Build configurations (dev, release, etc.)

---

## 4. Rust API Guidelines

| Resource | URL |
|----------|-----|
| API Guidelines | https://rust-lang.github.io/api-guidelines |
| GitHub Repository | https://github.com/rust-lang/api-guidelines |
| Checklist | https://rust-lang.github.io/api-guidelines/checklist.html |

**Relevance to Dock:** Guidelines for designing public APIs. wgpu-api's public interface should follow these guidelines for:
- Naming conventions (type names, function names, module names)
- Error handling patterns (Result types, error enums)
- Trait design (sealed traits, marker traits)
- Generic parameters (bounds, where clauses)
- Documentation standards

**Key Principles:**
- **Clear naming**: Types and functions should have predictable names
- **Good defaults**: Common cases should be easy, edge cases should be possible
- **Error clarity**: Errors should explain what went wrong and how to fix it
- **Consistency**: Follow existing patterns in the standard library

---

## 5. Rust Reference

| Resource | URL |
|----------|-----|
| The Rust Reference | https://doc.rust-lang.org/reference |
| Types | https://doc.rust-lang.org/reference/types.html |
| Expressions | https://doc.rust-lang.org/reference/expressions.html |
| Patterns | https://doc.rust-lang.org/reference/patterns.html |
| Traits | https://doc.rust-lang.org/reference/items/traits.html |
| Modules | https://doc.rust-lang.org/reference/items/modules.html |
| Lifetimes | https://doc.rust-lang.org/reference/lifetime-elision.html |
| Unsafe Rust | https://doc.rust-lang.org/reference/unsafe.html |

**Relevance to Dock:** The definitive specification of the Rust language. Use when:
- Understanding lifetime elision rules
- Checking pattern matching semantics
- Understanding trait resolution
- Verifying unsafe code correctness
- Understanding module resolution

---

## 6. Clippy Documentation

| Resource | URL |
|----------|-----|
| Clippy | https://doc.rust-lang.org/clippy |
| Lint List | https://doc.rust-lang.org/clippy/lints.html |
| Configuration | https://doc.rust-lang.org/clippy/configuration.html |
| Contributing | https://doc.rust-lang.org/clippy/development.html |

**Relevance to Dock:** Clippy is Rust's linter. Run `cargo clippy` to catch:
- Common mistakes (e.g., `map` then `clone` instead of `cloned`)
- Performance issues (e.g., unnecessary allocations)
- Style violations (e.g., redundant code)
- Correctness issues (e.g., potential panics)

**Usage:**
```bash
cargo clippy --all-targets --all-features -- -D warnings
```

**Key Lint Categories:**
- **Correctness**: Bugs and undefined behavior
- **Suspicious**: Code that is likely wrong
- **Style**: Code style and readability
- **Complexity**: Overly complex code patterns
- **Performance**: Performance anti-patterns

---

## 7. Rust Edition Guide

| Resource | URL |
|----------|-----|
| Rust 2021 Edition Guide | https://doc.rust-lang.org/edition-guide |
| Rust 2018 Edition Guide | https://doc.rust-lang.org/edition-guide/rust-2018 |
| Rust 2024 Edition (planned) | https://doc.rust-lang.org/edition-guide |

**Relevance to Dock:** Dock uses Rust 2021 edition. The edition guide covers:
- Changes between editions (2015 → 2018 → 2021)
- New language features (e.g., `async`/`await` in 2018)
- Disallowed patterns (e.g., disjoint capture in closures)
- Standard library changes

**Key Changes in Rust 2021:**
- Strict provenance for raw pointers
- `Box::leak` and `String::leak`
- `TryBlock` trait
- Disjoint capture in closures

---

## 8. Rust Release Notes

| Resource | URL |
|----------|-----|
| Release Notes | https://blog.rust-lang.org |
| Rust 1.70+ Releases | https://blog.rust-lang.org/2023 |
| Rust 1.80+ Releases | https://blog.rust-lang.org/2024 |
| Rust 1.90+ Releases | https://blog.rust-lang.org/2025 |

**Relevance to Dock:** Release notes document:
- New language features and standard library additions
- Deprecations and removals
- Compiler improvements and bug fixes
- Security advisories

**Why This Matters:**
- Dock depends on many crates that may use new Rust features
- Understanding deprecations helps avoid future breakage
- Security advisories may affect dependencies

---

## 9. Additional Rust Resources

### Rust Playground

| Resource | URL |
|----------|-----|
| Rust Playground | https://play.rust-lang.org |

**Relevance:** Quick testing of Rust code snippets without local setup. Useful for:
- Testing trait implementations
- Verifying lifetime behavior
- Sharing minimal reproduction cases

### Rust Forum

| Resource | URL |
|----------|-----|
| Users Forum | https://users.rust-lang.org |
| Zed Talk (Zed-based discussions) | https://rust-lang.zulipchat.com |

**Relevance:** Community support for Rust questions and best practices.

### Rust Unstable Book

| Resource | URL |
|----------|-----|
| The Unstable Book | https://doc.rust-lang.org/unstable-book |

**Relevance:** Documents unstable features behind feature gates. Useful for understanding upcoming language features that may stabilize.

---

## Engineering Workflow

### Before Making Changes

1. Read relevant sections of The Rust Book for language features in use.
2. Check the Rust Reference for precise semantics of constructs.
3. Review API Guidelines for public interface changes.

### After Making Changes

1. Run `cargo check` to verify compilation.
2. Run `cargo clippy` to catch lint violations.
3. Run `cargo test` to verify tests pass.
4. Check release notes for any relevant deprecations.

### Code Quality Checklist

- [ ] Code compiles without warnings (`cargo check`)
- [ ] Clippy passes without errors (`cargo clippy`)
- [ ] Tests pass (`cargo test`)
- [ ] Public APIs follow API Guidelines
- [ ] Documentation comments are present on public items
- [ ] Error types are descriptive and actionable
- [ ] Unsafe code is minimal and well-documented

---

## Version Compatibility

| Component | Version | Notes |
|-----------|---------|-------|
| Rust Edition | 2021 | Dock uses Rust 2021 edition |
| MSRV | Check Cargo.toml | Minimum Supported Rust Version |
| Toolchain | rustup | Manage Rust versions via rustup |

**Note:** Check `Cargo.toml` for the `rust-version` field to find the MSRV. Do not use language features newer than the MSRV without updating it.
