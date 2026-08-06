# Dock

A lightweight, high-performance Wayland dock written in Rust, designed for modern Wayland compositors.

![Dock Demo](dock.gif)

## Features

- **Wayland Native**: Built with Layer Shell and Foreign Toplevel protocols for smooth integration.
- **Fast & Lightweight**: Low resource consumption with seamless animations and hover effects.
- **Desktop & Steam Integration**: Resolves application icons, `.desktop` files, and Steam game launcher shortcuts dynamically.
- **Built-in Utilities**: Window management, context menus, drag-and-drop support, and status indicators.

## Supported Compositors (Wayland)

`dock` works on any Wayland compositor supporting `wlr-layer-shell-unstable-v1` and `wlr-foreign-toplevel-management-unstable-v1`:

- **labwc** (Primary target / recommended)
- **Sway**[cite: 15]
- **Hyprland**[cite: 15]
- **Wayfire**[cite: 15]
- **river**[cite: 15]
- **COSMIC**[cite: 15]

## Prerequisites & Dependencies

### Rust Version
- **Cargo / Rustc**: `1.75.0` or higher (Rust Edition 2021 recommended)[cite: 15]

### System Dependencies

Ensure the following build tools and development libraries are installed on your system[cite: 15]:

- **Build Tools**: `make`, `gcc`, `pkg-config`[cite: 15]
- **Wayland Libraries**: `wayland-client`, `wayland-protocols`[cite: 15]
- **Input & Graphics**: `libxkbcommon`, `pixman`[cite: 15]
- **System Services**: `dbus`[cite: 15]

#### Installing Dependencies on Arch Linux:
```bash
sudo pacman -S --needed base-devel cargo wayland wayland-protocols libxkbcommon dbus