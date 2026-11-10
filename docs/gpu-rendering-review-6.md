# GPU Rendering Code Review — Round 6 (Physics Engine, Shaders, Wayland, Animation, Morphing, Primitives, Performance)

This is a continuation of the GPU rendering code review. Rounds 1–5 covered `renderer.rs`, `backend.rs`, `dock.rs`, `dashed_block.rs`, `scene.rs`, `command.rs`, `video.rs`, `gpu.rs`, `animation.rs`, `render.rs`, `date_time_block.rs`, `draw.rs`, `texture.rs`, `layout/*`, `interaction/*`, `batching.rs`, `clip.rs`, `frame.rs`, `text_render.rs`, `geometry.rs`, `color.rs`, `brush.rs`, `image.rs`, `gif.rs`, `transform.rs`, `viewport.rs`, `surface.rs`, `error.rs`, and `lib.rs` (issues 1–120). This round covers `physics/*`, `shaders.rs`, `wayland.rs`, `animation.rs`, `morphing.rs`, `primitives.rs`, and `perf.rs` (issues 121–155).

---

## Critical Issues

### 121. physics/world.rs resolve_collisions clones all bodies every frame (world.rs:407–408)
**File:** `wgpu-api/src/physics/world.rs`  
**Lines:** 407–408

```rust
let body_vec: Vec<PhysicsBody> = self.bodies.values().cloned().collect();
let body_ids: Vec<BodyId> = self.bodies.keys().copied().collect();
```

Every call to `resolve_collisions()` clones **all** `PhysicsBody` instances into a new `Vec<PhysicsBody>`, and all `BodyId` keys into a separate `Vec<BodyId>`. Then for each collision pair, it clones the two colliding bodies again (lines 417–418). For a dock with 20 icons, this allocates 20 full body structs + 20 BodyIds + 4 more per collision pair, every physics step (120 Hz).

**Impact:** Per-frame heap allocation in the physics hot path. At 120 Hz with 20 bodies, this is ~2,400 allocations/sec. Violates Core Engineering Priority #2 (lowest possible system RAM usage) and #1 (lowest possible CPU usage).

**Fix:** Use references instead of clones. The broad phase only needs bounding rects, not full body clones. Store `(BodyId, PhysicsBody)` pairs or use indices into the HashMap.

---

### 122. physics/collision.rs broad_phase is O(n²) with no spatial partitioning (collision.rs:251–269)
**File:** `wgpu-api/src/physics/collision.rs`  
**Lines:** 251–269

```rust
pub fn broad_phase(bodies: &[PhysicsBody]) -> Vec<(usize, usize)> {
    let mut pairs = Vec::new();
    let len = bodies.len();
    for i in 0..len {
        for j in (i + 1)..len {
            // ...
        }
    }
    pairs
}
```

The broad phase checks every pair of bodies. For N bodies, this is N*(N-1)/2 checks. A dock with 50 icons = 1,225 pair checks per physics step at 120 Hz = 147,000 checks/sec. No spatial hash grid, sweep-and-prune, or BV tree is used.

**Impact:** Collision detection scales quadratically. A dock with 100 icons would perform 4,950 pair checks per step. While dock icon counts are typically low (<50), this is still wasteful when most icons are far apart.

**Fix:** Use a spatial hash grid or sweep-and-prune algorithm. The `spatial_index.rs` module exists but is not integrated into the broad phase.

---

### 123. physics/body.rs apply_force accumulates acceleration incorrectly (body.rs:589–599)
**File:** `wgpu-api/src/physics/body.rs`  
**Lines:** 589–599

```rust
pub fn apply_force(&mut self, force: Vector2D, dt: f32) {
    // ...
    self.acceleration.x += force.x * self.inv_mass;
    self.acceleration.y += force.y * self.inv_mass;
    self.velocity.x += self.acceleration.x * dt;
    self.velocity.y += self.acceleration.y * dt;
}
```

`apply_force` adds to `acceleration` and then immediately integrates it into velocity. But if `apply_force` is called multiple times per step (e.g., spring force + field force + gravity), the acceleration from previous calls is integrated again. The first force's acceleration gets integrated N times where N is the number of force applications.

Example: spring applies F₁, velocity += a₁*dt. Then field applies F₂, acceleration is now a₁+a₂, velocity += (a₁+a₂)*dt. The spring's acceleration a₁ was integrated twice.

**Impact:** Forces are over-applied when multiple force types act on the same body. The simulation produces incorrect velocities that grow faster than expected.

**Fix:** Separate force accumulation from integration. Add force to acceleration without integrating, then integrate once at the end of the step (which `integrate_all` already does via `reset_acceleration`).

---

### 124. shaders.rs uniform buffer uses mat2x2 + vec2 instead of 3x3 matrix (shaders.rs:29–32, 83–86)
**File:** `wgpu-api/src/shaders.rs`  
**Lines:** 29–32

```wgsl
struct Uniforms {
    transform: mat2x2<f32>,
    offset: vec2<f32>,
}
```

The shader uniform is a 2×2 matrix plus a 2D offset. This represents only **scale + rotation + translation** — no skew, no perspective, no 3×3 transform. The `Transform` struct in `transform.rs` uses a full 3×3 matrix (`Matrix3` with 9 elements). The uniform buffer cannot represent the full transform.

**Impact:** Any transform with skew or non-uniform scaling combined with rotation will render incorrectly. The 3×3 → 2×2+vec2 conversion loses information.

**Fix:** Use `mat3x2<f32>` (6 bytes) or `mat3x3<f32>` (9 bytes) in the shader uniform to match the Rust `Matrix3` type.

---

### 125. wayland.rs process_input_event FocusChange has tautology bug (wayland.rs:801–806)
**File:** `wgpu-api/src/wayland.rs`  
**Lines:** 801–806

```rust
InputEvent::FocusChange(focused) => {
    for window in self.windows.values_mut() {
        window.on_focus_changed(*focused && window.id == window.id);
    }
}
```

`window.id == window.id` is always `true`. This means all windows get the same focus state. If `focused` is `true`, all windows are focused. If `false`, all windows lose focus. The intent was likely to focus only the target window, but the event doesn't carry a window ID.

**Impact:** Focus state is incorrect — all windows share the same focus state. Keyboard input routing based on focus will be wrong.

**Fix:** Make `InputEvent::FocusChange` carry a `WaylandWindowId` parameter, or route focus to the focused window only.

---

## High Severity Issues

### 126. physics/timestep.rs run_fixed_step mutates world config (timestep.rs:304–305)
**File:** `wgpu-api/src/physics/timestep.rs`  
**Lines:** 304–305

```rust
let original_dt = world.config.timestep;
world.config.timestep = self.config.fixed_dt;
```

`run_fixed_step` temporarily overwrites `world.config.timestep` with the simulator's `fixed_dt`. If the world's timestep was already set to the same value, this is a no-op. But if code between steps reads `world.config.timestep`, it gets the wrong value. The `original_dt` is saved but never restored in the visible code.

**Impact:** The world's timestep config is silently modified. If other code depends on `world.config.timestep`, it sees the simulator's value, not the original.

**Fix:** Pass `fixed_dt` as a parameter to `world.step()` instead of mutating config. Or ensure `original_dt` is restored after the step.

---

### 127. physics/timestep.rs velocity_iterations loop re-applies all forces (timestep.rs:308–310)
**File:** `wgpu-api/src/physics/timestep.rs`  
**Lines:** 308–310

```rust
for _ in 0..self.config.velocity_iterations {
    world.apply_springs(self.config.fixed_dt);
    world.apply_fields(self.config.fixed_dt);
```

The velocity iterations loop re-applies springs and fields multiple times per step. Each iteration adds the same spring force and field force. With 3 velocity iterations, each force is applied 3 times — tripling the effective force.

**Impact:** Forces are multiplied by the number of velocity iterations. With default 3 iterations, springs and fields are 3× stronger than intended. The simulation is unstable and produces exaggerated motion.

**Fix:** Velocity iterations should only resolve constraints (collisions), not re-apply forces. Move force application outside the iteration loop.

---

### 128. animation.rs CubicBezier easing ignores x coordinates (animation.rs:97–101)
**File:** `wgpu-api/src/animation.rs`  
**Lines:** 97–101

```rust
Self::CubicBezier(_x1, y1, _x2, y2) => {
    let one_minus_t = 1.0 - t;
    one_minus_t * one_minus_t * one_minus_t * y1
        + 3.0 * one_minus_t * one_minus_t * t * y2
```

The cubic bezier easing takes 4 parameters `(x1, y1, x2, y2)` defining two control points. But the implementation ignores `x1` and `x2` (prefixed with `_`), and uses only `y1` and `y2`. A proper cubic bezier easing solves for the parameter `t` given the input x, then computes y. This implementation treats it as a simple polynomial, not a bezier curve.

**Impact:** `CubicBezier(0.25, 0.1, 0.25, 1.0)` (standard ease-out) produces the same result as `CubicBezier(0.0, 0.1, 1.0, 1.0)` — the x coordinates are meaningless. The easing doesn't match standard CSS bezier curves.

**Fix:** Implement proper cubic bezier parameterization with Newton-Raphson iteration to solve for t given x.

---

### 129. animation.rs Elastic easing uses Back formula (animation.rs:90–95)
**File:** `wgpu-api/src/animation.rs`  
**Lines:** 90–95

```rust
Self::Elastic => {
    if t == 0.0 || t == 1.0 {
        t
    } else {
        t * t * t * ((1.70158 + 1.0) * t - 1.70158)
    }
}
```

The `Elastic` easing variant uses the same formula as `Back` easing (`t³ * ((s+1)*t - s)` where s=1.70158). True elastic easing uses sine/cosine to create oscillation: `sin(t * π * 1.5) * t` or similar.

**Impact:** `Elastic` easing produces the same overshoot behavior as `Back`, not the oscillating bounce expected from elastic easing. Code using `Elastic` gets wrong animation curves.

**Fix:** Implement proper elastic easing with sinusoidal oscillation.

---

### 130. animation.rs ContinuousAnimation::update returns 0.0 when inactive (animation.rs:832–835)
**File:** `wgpu-api/src/animation.rs`  
**Lines:** 832–835

```rust
pub fn update(&mut self, dt: f32) -> f32 {
    if !self.active {
        return 0.0;
    }
```

When a continuous animation is paused or cancelled, `update` returns `0.0` regardless of the animation type. For a rotation animation, this means the node snaps to rotation=0. For opacity, it snaps to 0 (invisible). For scale, it snaps to 0 (collapsed).

**Impact:** Pausing or cancelling a continuous animation causes the animated property to jump to 0, creating a visual glitch. The last known value should be returned instead.

**Fix:** Store the last evaluated value and return it when inactive.

---

### 131. animation.rs ParticleSystem emit_particle uses deterministic pseudo-random (animation.rs:1576–1601)
**File:** `wgpu-api/src/animation.rs`  
**Lines:** 1576–1601

```rust
fn emit_particle(&mut self, _seed: u64) {
    let lifetime = self.config.lifetime_range.0
        + (self.config.lifetime_range.1 - self.config.lifetime_range.0)
            * (self.particles.len() as f32 % 1.0);
```

Particle properties (lifetime, size, velocity) are computed from `self.particles.len() as f32 % 1.0`, which is always `0.0`. An integer modulo 1.0 is always 0. So every particle gets the minimum lifetime, minimum size, and minimum velocity.

**Impact:** All particles are identical — same lifetime, same size, same velocity. The particle system produces a single stream of identical particles instead of a varied spray.

**Fix:** Use a proper PRNG (e.g., `rand::thread_rng()`) or at least a hash-based random function.

---

### 132. perf.rs Stopwatch uses RefCell for interior mutability (perf.rs:47–52)
**File:** `wgpu-api/src/perf.rs`  
**Lines:** 47–52

```rust
pub struct Stopwatch {
    start: RefCell<Option<Instant>>,
    elapsed: RefCell<Duration>,
}
```

`Stopwatch` uses `RefCell` for interior mutability, allowing mutable access through shared references. This adds runtime borrow checking overhead to every timing call. If a borrow conflict occurs (e.g., nested `elapsed()` call during `stop()`), it panics.

**Impact:** Runtime panic risk in profiling code. The `RefCell` overhead adds ~10ns per call, which is significant for high-frequency timing.

**Fix:** Use `&mut self` for mutating methods, or use `Cell` for simple types.

---

### 133. primitives.rs Path horizontal_line_to and vertical_line_to may return early (primitives.rs:41–64)
**File:** `wgpu-api/src/primitives.rs`  
**Lines:** 41–64

```rust
pub fn horizontal_line_to(&mut self, x: f32) {
    let y = match self.commands.last() {
        Some(PathCommand::MoveTo(p)) => p.y,
        // ...
        _ => return,
    };
```

If the path is empty or the last command is `Close`, both `horizontal_line_to` and `vertical_line_to` return silently without adding any command. The caller has no way to detect the failure.

**Impact:** Silent failure when drawing horizontal/vertical lines on an empty path. The resulting path is incomplete and renders incorrectly.

**Fix:** Return `bool` indicating success, or document that these methods require a non-empty path.

---

### 134. morphing.rs GeometryAnimation doesn't validate duration (morphing.rs:84)
**File:** `wgpu-api/src/morphing.rs`  
**Line:** 84

```rust
duration: duration.max(0.001),
```

Duration is clamped to minimum 0.001 seconds, but negative durations are silently clamped. A negative duration likely indicates a bug in the caller.

**Impact:** Negative durations are silently corrected, masking bugs in animation setup code.

**Fix:** Warn or return `Result` for negative durations.

---

### 135. physics/springs.rs SpringConstraint uses BodyId::new(0) as sentinel (springs.rs:57)
**File:** `wgpu-api/src/physics/springs.rs`  
**Line:** 57

```rust
body_b: BodyId::new(0), // sentinel for fixed point
```

`BodyId::new(0)` is used as a sentinel value to indicate a fixed point. But `BodyId::new(0)` is a valid body ID — the world starts assigning IDs from 1, but nothing prevents a body from being created with ID 0.

**Impact:** If a body with ID 0 exists, the spring constraint treats it as a fixed point instead of a real body. This creates silent misbehavior.

**Fix:** Use `Option<BodyId>` instead of a sentinel value.

---

## Medium Severity Issues

### 136. physics/world.rs step() doesn't use config.max_substeps (world.rs:545–580)
**File:** `wgpu-api/src/physics/world.rs`  
**Lines:** 545–580

`PhysicsWorld::step()` runs exactly one simulation step. It ignores `config.max_substeps` entirely. The `step_substeps()` method exists but is separate. The `FixedTimestepSimulator` uses its own `max_substeps` from `PhysicsConfig`, not from `PhysicsWorldConfig`.

**Impact:** Two separate `max_substeps` configs exist with no connection. The world's config is ignored.

---

### 137. physics/world.rs step() doesn't use config.solver_iterations (world.rs:545–580)
**File:** `wgpu-api/src/physics/world.rs`  
**Lines:** 545–580

`PhysicsWorldConfig::solver_iterations` is set to 8 by default, but `step()` runs collision resolution once, not `solver_iterations` times. The `FixedTimestepSimulator` has its own `velocity_iterations` and `position_iterations` that control iteration count.

**Impact:** `solver_iterations` in `PhysicsWorldConfig` is a dead config value. The actual iteration count comes from `FixedTimestepSimulator::config`.

---

### 138. physics/body.rs put_to_sleep zeros velocity unconditionally (body.rs:616–622)
**File:** `wgpu-api/src/physics/body.rs`  
**Lines:** 616–622

```rust
pub fn put_to_sleep(&mut self) {
    if self.allow_sleep {
        self.is_sleeping = true;
        self.velocity = Vector2D::ZERO;
        self.angular_velocity = 0.0;
    }
}
```

Putting a body to sleep zeros its velocity. But `should_sleep` already checks that velocity is below threshold. The velocity zeroing is redundant and loses the small residual velocity that might be needed for interpolation.

**Impact:** Minor — the velocity is already near-zero when sleeping. But interpolation between the last awake state and the slept state might show a small jump.

---

### 139. physics/collision.rs circle_vs_circle rejects zero-distance collision (collision.rs:150)
**File:** `wgpu-api/src/physics/collision.rs`  
**Line:** 150

```rust
if dist_sq < min_dist * min_dist && dist_sq > 0.0001 {
```

If two circles are at exactly the same position (`dist_sq == 0.0`), the collision is rejected because `0.0 > 0.0001` is false. The bodies pass through each other.

**Impact:** Perfectly overlapping circles don't collide. This is an edge case but can occur when bodies are created at the same position.

---

### 140. physics/collision.rs aabb_vs_circle_internal rejects zero-distance (collision.rs:203)
**File:** `wgpu-api/src/physics/collision.rs`  
**Line:** 203

```rust
if dist_sq < radius * radius && dist_sq > 0.0001 {
```

Same issue as #139 — if the circle center is exactly at the closest point on the AABB, the collision is rejected.

---

### 141. wayland.rs WaylandConnection doesn't track which window is focused (wayland.rs:791–819)
**File:** `wgpu-api/src/wayland.rs`  
**Lines:** 791–819

`process_input_event` routes events to "the focused window" by iterating all windows and checking `window.focused`. But `FocusChange` sets focus on all windows (issue #125). There's no mechanism to set which specific window receives focus.

**Impact:** Input events are routed to all windows or no windows, depending on the buggy focus state.

---

### 142. wayland.rs InputEvent::Configure routes to focused window only (wayland.rs:793–799)
**File:** `wgpu-api/src/wayland.rs`  
**Lines:** 793–799

```rust
InputEvent::Configure { size, scale_factor } => {
    for window in self.windows.values_mut() {
        if window.focused {
            window.on_resize(*size, *scale_factor);
        }
    }
}
```

Resize events are only applied to the focused window. But Wayland sends configure events for all visible windows, not just the focused one. A non-focused window that resizes won't be updated.

**Impact:** Non-focused windows don't resize correctly. The event should carry a window ID.

---

### 143. animation.rs Animation::update doesn't clamp progress before sample (animation.rs:290)
**File:** `wgpu-api/src/animation.rs`  
**Line:** 290

```rust
self.progress += dt / self.duration;
```

If `dt` is very large (e.g., application was suspended for minutes), `self.progress` can become a very large number. While the repeat logic handles `progress >= 1.0`, the `fract()` call on a very large float can lose precision.

**Impact:** After a long suspension, animation progress may jump to a random frame due to floating-point precision loss in `fract()`.

---

### 144. animation.rs AnimatedNode::update doesn't deduplicate properties (animation.rs:921–926)
**File:** `wgpu-api/src/animation.rs`  
**Lines:** 921–926

```rust
pub fn update(&mut self, dt: f32) -> Vec<(AnimatedProperty, f32)> {
    self.animations
        .iter_mut()
        .filter(|a| a.active)
        .map(|a| (a.property, a.update(dt)))
        .collect()
}
```

If two animations target the same property (e.g., two opacity animations), both values are returned. The caller applies them in order, so the last one wins. The first animation's value is silently overwritten.

**Impact:** Conflicting animations on the same property produce unpredictable results. The last animation in the list wins.

---

### 145. animation.rs ContinuousAnimation::pause sets active=false, not state (animation.rs:865–867)
**File:** `wgpu-api/src/animation.rs`  
**Lines:** 865–867

```rust
pub fn pause(&mut self) {
    self.active = false;
}
```

`ContinuousAnimation` uses `active: bool` for pause/resume, while `Animation` uses `AnimationState::Paused`. The two animation types use different pause mechanisms. `ContinuousAnimation` has no way to distinguish between "paused" and "cancelled".

**Impact:** Inconsistent pause semantics between animation types. Code that checks for pause state can't distinguish pause from cancel for continuous animations.

---

### 146. physics/springs.rs compute_force divides by near-zero length (springs.rs:69–77)
**File:** `wgpu-api/src/physics/springs.rs`  
**Lines:** 69–77

```rust
let current_length = (dx * dx + dy * dy).sqrt();
if current_length < 0.0001 {
    return Vector2D::ZERO;
}
let nx = dx / current_length;
let ny = dy / current_length;
```

The threshold `0.0001` is very small. If two bodies are very close but not exactly at the same position, the division produces a valid but potentially large force due to numerical instability.

**Impact:** Near-zero distance produces large forces, causing instability. The threshold should be larger or the force should be clamped.

---

### 147. physics/timestep.rs FixedTimestepSimulator doesn't clean up removed bodies (timestep.rs:286–296)
**File:** `wgpu-api/src/physics/timestep.rs`  
**Lines:** 286–296

```rust
fn save_previous_states(&mut self, world: &PhysicsWorld) {
    for (id, body) in world.iter_bodies() {
        self.previous_states.insert(*id, PreviousState { ... });
    }
}
```

`save_previous_states` inserts all current bodies but never removes entries for deleted bodies. If a body is removed from the world, its previous state remains in the HashMap forever.

**Impact:** Memory leak — `previous_states` grows indefinitely as bodies are added and removed. Old BodyIds accumulate stale data.

**Fix:** Remove entries for bodies no longer in the world, or use `retain()` after saving.

---

### 148. shaders.rs BLUR_FRAGMENT_SHADER uses textureDimensions on f32 texture (shaders.rs:157)
**File:** `wgpu-api/src/shaders.rs`  
**Line:** 157

```wgsl
let texel_size = 1.0 / textureDimensions(texture_view);
```

`textureDimensions()` returns `vec2<u32>`. Dividing `1.0` (f32) by `vec2<u32>` requires implicit conversion. In WGSL, this may not compile depending on the shader version. The result should be `vec2<f32>(1.0) / vec2<f32>(textureDimensions(texture_view))`.

**Impact:** Shader compilation may fail on some wgpu backends due to type mismatch.

---

### 149. shaders.rs vertex shader positions are in clip space, not screen space (shaders.rs:39)
**File:** `wgpu-api/src/shaders.rs`  
**Line:** 39

```wgsl
let pos = uniforms.transform * input.position + uniforms.offset;
output.clip_position = vec4<f32>(pos, 0.0, 1.0);
```

The vertex shader takes positions and applies a 2×2 transform + offset, then directly outputs to clip space. This means vertex positions must already be in clip space [-1, +1] before the transform. The renderer must convert physical pixel coordinates to clip space before creating vertices.

**Impact:** The transform uniform must encode the pixel-to-clip conversion. If the uniform buffer isn't updated correctly, everything renders at the wrong scale or position.

---

### 150. wayland.rs WaylandWindowConfig default size is 800×600 (wayland.rs:190)
**File:** `wgpu-api/src/wayland.rs`  
**Line:** 190

```rust
size: Size::new(800.0, 600.0),
```

The default window size is 800×600. For a dock, this is enormous. A dock is typically a thin bar (e.g., 800×64). Creating a dock window with default config produces a huge window.

**Impact:** Dock windows created with default config are the wrong size. The caller must always override the size.

---

## Low Severity Issues

### 151. physics/body.rs PhysicsBody derives Clone but is large (body.rs:399)
**File:** `wgpu-api/src/physics/body.rs`  
**Line:** 399

```rust
#[derive(Debug, Clone)]
pub struct PhysicsBody {
```

`PhysicsBody` is ~100+ bytes with many fields. Deriving `Clone` encourages copying, which is what causes issue #121. The `Clone` impl copies all fields including `Option<String>` for `tag`.

**Impact:** Encourages expensive clones in performance-critical code.

---

### 152. physics/world.rs PhysicsWorld uses HashMap for bodies (world.rs:128)
**File:** `wgpu-api/src/physics/world.rs`  
**Line:** 128

```rust
bodies: HashMap<BodyId, PhysicsBody>,
```

HashMap lookup is O(1) average but has high constant factor due to hashing. For iteration (which is what `iter_bodies` does), `Vec` or `IndexMap` would be faster and more cache-friendly.

**Impact:** Iteration over bodies is slower than necessary due to HashMap's pointer-indirected layout.

---

### 153. animation.rs Easing::EaseInOut formula may be incorrect (animation.rs:65–70)
**File:** `wgpu-api/src/animation.rs`  
**Lines:** 65–70

```rust
Self::EaseInOut => {
    if t < 0.5 {
        2.0 * t * t
    } else {
        1.0 - (-2.0 * t + 2.0).powi(2) / 2.0
    }
}
```

The second half: `1.0 - (-2.0*t + 2.0)² / 2.0`. At t=0.5: `1.0 - (-1.0)²/2.0 = 1.0 - 0.5 = 0.5` ✓. At t=1.0: `1.0 - (0.0)²/2.0 = 1.0` ✓. But the derivative at t=0.5 from the left is `4*0.5 = 2.0`, and from the right is `(-2.0)*(-2.0*0.5+2.0)/2.0 * (-1) = ...` — the derivative is discontinuous at t=0.5.

**Impact:** EaseInOut has a visible "kink" at the midpoint where the speed changes abruptly.

---

### 154. perf.rs Stopwatch doesn't implement Send/Sync (perf.rs:46–52)
**File:** `wgpu-api/src/perf.rs`  
**Lines:** 46–52

`Stopwatch` uses `RefCell` which is not `Send` or `Sync`. This means `Stopwatch` can't be shared across threads. If profiling code runs on multiple threads, each thread needs its own stopwatch.

**Impact:** Can't use `Stopwatch` in multi-threaded profiling scenarios.

---

### 155. primitives.rs Path::arc doesn't validate angle range (primitives.rs:130–145)
**File:** `wgpu-api/src/primitives.rs`  
**Lines:** 130–145

The `arc` method draws an arc from `start_angle` to `end_angle`. If the angles are not normalized, the arc may draw the wrong direction or multiple full rotations.

**Impact:** Arcs with unnormalized angles render incorrectly.

---

## Verification and corrections

The findings above describe an earlier code snapshot. This section supersedes unsupported claims and severity labels where noted; the list contains 35 numbered entries, but the original severity totals should not be treated as verified.

| Finding | Current assessment |
|---|---|
| 121 | **Fixed.** Collision resolution now collects body IDs and bounds instead of cloning every body and each pair. The metadata and candidate-pair vectors still allocate. |
| 122 | **Open performance concern.** Broad phase remains O(n²); the report does not establish that this is a measured bottleneck for typical dock workloads. |
| 123 | **Fixed.** Each applied force contributes its own `F/m * dt` to velocity; accumulated acceleration is retained for diagnostics and is not reintegrated on every call. |
| 124 | **Incorrect.** A 2×2 linear transform plus offset represents any 2D affine transform, including skew and nonuniform scale. Perspective is not represented by the CPU `Transform` either. |
| 125 | **Fixed.** Focus events identify a target window and only that window's focus state is changed. |
| 126 | **Obsolete/incorrect for current code.** `run_fixed_step` no longer temporarily mutates `PhysicsWorldConfig`. |
| 127 | **Fixed.** Springs, fields, and gravity are applied once per fixed step, outside the collision-position iteration loop. |
| 128–131 | **Fixed.** Cubic-Bezier easing solves the x parameter; Elastic uses an oscillating curve; paused continuous animations preserve their evaluated value; particle variation uses a deterministic hash-based sample. |
| 132 | **Unsupported.** The cited timing overhead is unmeasured, and the methods do not retain overlapping `RefCell` borrows in ordinary use. Thread sharing is a separate limitation (#154). |
| 133–134 | **Not demonstrated defects.** Requiring a current point for horizontal/vertical path commands and clamping animation duration are API/design choices; no incorrect output is established. |
| 135 | **Fixed.** Fixed-point springs store `Some(point)` and use `Option<BodyId>` for the second endpoint, removing the `BodyId(0)` sentinel. |
| 136 | **Not a defect.** `PhysicsWorld::step` deliberately runs one step; substeps are exposed separately and the fixed-step simulator owns its accumulator. |
| 137 | **Configuration concern remains.** `PhysicsWorldConfig::solver_iterations` is not consumed by `PhysicsWorld::step`; the simulator's position-iteration setting controls collision repetitions. The similarly named velocity setting does not currently run a separate velocity solver. |
| 138 | **Not demonstrated.** Sleep intentionally clears sub-threshold velocity; no visible interpolation failure was established. |
| 139 | **Fixed.** Coincident circles now produce a stable fallback normal and a collision. |
| 140 | **Incorrect.** A circle center inside the AABB, including zero closest-point distance, is handled by the containment branch. |
| 141 | **Duplicate of 125; fixed with it.** |
| 142 | **Fixed.** Configure events carry a window ID and resize that specific window, regardless of focus. |
| 143 | **Unverified edge case.** Very large elapsed deltas can reduce `f32` repeat precision, but the report does not demonstrate a random-frame result under supported inputs. |
| 144–145 | **Not demonstrated defects.** Animations are returned in insertion order for the caller to resolve, and `active` provides pause/resume behavior without a separate cancellation state. |
| 146 | **Incorrect.** The near-zero guard runs before normalization; the cited division by an effectively zero distance does not occur. |
| 147 | **Fixed.** Previous-state entries for removed bodies are pruned before saving the next snapshot. |
| 148 | **Fixed.** Texture dimensions are explicitly converted to `vec2<f32>` before floating-point division. |
| 149 | **Contract-dependent, not a standalone defect.** The vertex shader consumes clip-space positions; the report does not identify a producer that passes pixel-space coordinates to it. |
| 150 | **Not a defect by itself.** `WaylandWindowConfig` is generic, and its caller may override the default size for a dock surface. |
| 151–152 | **Optimization suggestions, not defects.** `Clone` is only costly when used, and `HashMap` is a reasonable keyed body store absent profiling evidence. |
| 153 | **Incorrect.** The listed EaseInOut curve has a continuous first derivative at `t = 0.5`. |
| 154 | **Partly correct limitation.** `RefCell` makes `Stopwatch` not `Sync`; it can still be moved between threads (`Send`). No cross-thread sharing requirement is established. |
| 155 | **Not demonstrated.** Arc angles are accepted as radians; unnormalized values are valid inputs unless a particular renderer imposes a narrower contract. |

Targeted tests passed for physics body (19), fixed timestep (25), springs (12), animation (30), Wayland (32), and the coincident-circle regression. The collision module's remaining `test_resolve_collision` test still fails at its pre-existing assertion that a particular collision changes `a.velocity.x` or `a.position.x`; the other 11 collision tests passed.

---

## Summary of Issues by Severity

| # | Severity | Issue | Impact |
|---|----------|-------|--------|
| 121 | **Critical** | resolve_collisions clones all bodies | Per-frame heap allocation at 120 Hz |
| 122 | **Critical** | broad_phase O(n²) no spatial partitioning | Quadratic collision checks |
| 123 | **Critical** | apply_force accumulates acceleration wrong | Forces over-applied, incorrect physics |
| 124 | **Critical** | Shader uniform mat2x2 not 3x3 | Transform info loss |
| 125 | **Critical** | FocusChange tautology bug | All windows share focus state |
| 126 | **High** | run_fixed_step mutates world config | Silent config corruption |
| 127 | **High** | velocity_iterations re-applies forces | Forces multiplied by iteration count |
| 128 | **High** | CubicBezier ignores x coordinates | Easing doesn't match CSS bezier |
| 129 | **High** | Elastic easing uses Back formula | Wrong animation curve |
| 130 | **High** | ContinuousAnimation returns 0 when paused | Visual glitch on pause |
| 131 | **High** | ParticleSystem pseudo-random is always 0 | All particles identical |
| 132 | **High** | Stopwatch uses RefCell | Runtime panic risk |
| 133 | **Medium** | Path horizontal/vertical line silent fail | Incomplete paths |
| 134 | **Medium** | GeometryAnimation clamps negative duration | Masks caller bugs |
| 135 | **Medium** | SpringConstraint sentinel BodyId(0) | Silent misbehavior |
| 136 | **Medium** | step() ignores max_substeps config | Dead config value |
| 137 | **Medium** | step() ignores solver_iterations config | Dead config value |
| 138 | **Medium** | put_to_sleep zeros velocity | Minor interpolation jump |
| 139 | **Medium** | circle_vs_circle rejects zero distance | Overlapping circles don't collide |
| 140 | **Medium** | aabb_vs_circle rejects zero distance | Same as #139 |
| 141 | **Medium** | WaylandConnection doesn't track focused window | Input routing broken |
| 142 | **Medium** | Configure routes to focused only | Non-focused windows don't resize |
| 143 | **Medium** | Animation progress precision loss | Random frame after suspension |
| 144 | **Medium** | AnimatedNode doesn't deduplicate properties | Conflicting animations |
| 145 | **Medium** | ContinuousAnimation pause vs cancel | Inconsistent pause semantics |
| 146 | **Medium** | Spring force near-zero division | Numerical instability |
| 147 | **Medium** | FixedTimestepSimulator leaks previous states | Memory leak on body removal |
| 148 | **Medium** | BLUR shader type mismatch | Shader compilation failure |
| 149 | **Low** | Vertex shader clip space assumption | Requires correct uniform setup |
| 150 | **Low** | Default window size 800×600 | Wrong size for dock |
| 151 | **Low** | PhysicsBody derives Clone | Encourages expensive copies |
| 152 | **Low** | HashMap for bodies | Slower iteration |
| 153 | **Low** | EaseInOut derivative discontinuity | Visible kink at midpoint |
| 154 | **Low** | Stopwatch not Send/Sync | Can't share across threads |
| 155 | **Low** | Path::arc doesn't validate angles | Wrong arc rendering |

---

## Cumulative Statistics

| Round | File | Issues | Critical | High | Medium | Low |
|-------|------|--------|----------|------|--------|-----|
| 1 | gpu-rendering-review.md | 1–12 | 1 | 2 | 2 | 7 |
| 2 | gpu-rendering-review-2.md | 13–32 | 3 | 4 | 6 | 7 |
| 3 | gpu-rendering-review-3.md | 33–60 | 3 | 5 | 6 | 6 |
| 4 | gpu-rendering-review-4.md | 61–95 | 4 | 13 | 21 | 7 |
| 5 | gpu-rendering-review-5.md | 96–120 | 5 | 7 | 10 | 5 |
| 6 | gpu-rendering-review-6.md | 121–155 | 5 | 8 | 16 | 6 |
| **Total** | | **155** | **21** | **39** | **61** | **38** |
