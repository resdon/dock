# Dock

[![Rust CI](https://github.com/resdon/dock/actions/workflows/rust.yml/badge.svg?branch=1.3.8)](https://github.com/resdon/dock/actions/workflows/rust.yml)

A lightweight, high-performance Wayland dock written in Rust

![Dock Demo](showcase.gif)

## Features

- **Auto-hide**: Remains hidden until the cursor hovers near the screen edge, sliding smoothly into view.
- **Smart focus**: Click an application icon to switch focus, bring its window to the foreground, or background.
- **Window list**: Hover over an icon to see a list of open windows. Use the scroll wheel to cycle through them, or click the red box on a preview to close that window.
- **Drag&Drop**: Drag files directly onto dock icons to pass context or launch them with specific applications.
- **Context Menu**: Right-click an icon to access app-specific actions (e.g., "Open in Incognito Mode").
- **Pins**: Pin or unpin applications, and drag-and-drop icons to reorder them anywhere on the dock.
- **Wayland Native**: Built with Layer Shell and Foreign Toplevel protocols for smooth integration.
- **Fast & Transparent**: Low resource consumption with seamless transparent animations.
- **RAM & CPU**: ~35–40 MB RAM | Single-thread CPU: 0.6% idle, ~25% active average (60% peak).
- **Desktop & Steam Integration**: Resolves application icons, `.desktop` files, and Steam game launcher shortcuts dynamically.
- **D-Bus Support**: Integrates with system protocols for dynamic window management, drag-and-drop handling, and running status indicators.

## Supported Compositors (Wayland)

`dock` works on any Wayland compositor supporting `wlr-layer-shell-unstable-v1` and `wlr-foreign-toplevel-management-unstable-v1`:

- **labwc** (Primary target / recommended)
- **Sway**
- **Hyprland**
- **Wayfire**
- **river**
- **COSMIC**

## Prerequisites & Dependencies

### Rust Version
- **Cargo / Rustc**: `1.75.0` or higher (Rust Edition 2021 recommended)

### System Dependencies

Ensure the following build tools and development libraries are installed on your system:

- **Build Tools**: `make`, `gcc`, `pkg-config`
- **Wayland Libraries**: `wayland-client`, `wayland-protocols`
- **Input & Graphics**: `libxkbcommon`, `pixman`, `cairo`, `glib2`, `mesa` (EGL/GLES)
- **System Services**: `dbus`

#### Installing Dependencies on Arch Linux:
```bash
sudo pacman -S --needed base-devel cargo wayland wayland-protocols libxkbcommon fontconfig dbus cairo glib2 mesa
```

#### Installing Dependencies on Ubuntu / Debian:
```bash
sudo apt update
sudo apt install -y build-essential cargo pkg-config libwayland-dev wayland-protocols libxkbcommon-dev libfontconfig1-dev libdbus-1-dev libglib2.0-dev libcairo2-dev libegl1-mesa-dev libgles2-mesa-[...]
```

## Installation

### Pre-compiled Binary
Download and extract the [latest release archive](https://github.com/resdon/dock/releases/latest/download/dock-linux-amd64.tar.gz):
```bash
tar -xzf dock-linux-amd64.tar.gz
cd dock-linux-amd64
make install
```
Or download via terminal:
```bash
curl -sSL https://github.com/resdon/dock/releases/latest/download/dock-linux-amd64.tar.gz | tar -xz
cd dock-linux-amd64
make install
```

### Building From Source

### 1. Clone the Repository
```bash
git clone https://github.com/resdon/dock.git
cd dock
```

### 2. Build and Install
Compile the binary in release mode and install it to system PATH (`/usr/local/bin`):

```bash
make install
```

*Alternatively, build manually via Cargo:*
```bash
cargo build --release
```

### 3. Launching the Dock

#### Run in Background (CLI):
```bash
nohup dock &
```

#### Autostarting on `labwc`:
Add the following line to `~/.config/labwc/autostart`:
```bash
dock &
```

#### Autostarting on `Sway` / `Hyprland`:
- **Sway** (`~/.config/sway/config`):
  ```sway
  exec dock
  ```
- **Hyprland** (`~/.config/hypr/hyprland.conf`):
  ```ini
  exec-once = dock
  ```

## Repository Structure

```text
dock/
├── assets/          # Bundled fonts, icons, and demo images used by the project and runtime
├── scripts/         # Helper scripts: launcher, icon listing, packaging and development helpers
├── src/             # Core Rust source files
│   ├── app/         # Program entry, main event loop, and overall application state
│   ├── cache/       # Icon indexing, on-disk cache, and state persistence
│   ├── geometry/    # Layout maths, sizing, and bounding boxes for dock items
│   ├── graphics/    # Animation, compositing helpers, visual effects and transitions
│   ├── handlers/    # Event handlers for menus, window list, and user actions
│   ├── interaction/ # Proximity detection, hover logic, and motion handling
│   ├── pointer/     # Pointer input handling: clicks, drags and scroll interactions
│   ├── render/      # Low-level drawing routines, font rendering, and frame submission
│   ├── resolvers/   # `.desktop` file parsing and Steam/launcher app resolving
│   └── services/    # Compositor integration, display/monitor detection and D-Bus services
├── Cargo.toml       # Cargo package manifest and dependency declarations
├── Makefile         # Build, test, and install targets
└── PKGBUILD         # Arch Linux packaging script
```

## License

Distributed under the MIT License.
