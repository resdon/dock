# Maintainer: resdon
pkgname=dock
pkgver=1.5.5
pkgrel=1
pkgdesc="A hardware-accelerated Wayland dock application"
arch=('x86_64')
license=('custom')
depends=(
  'wayland'
  'wayland-protocols'
  'libxkbcommon'
  'fontconfig'
  'dbus'
  'cairo'
  'glib2'
  'mesa'
  'gcc-libs'
  'pipewire'
)
makedepends=(
  'rust'
  'cargo'
  'pkgconf'
)
# Source tarball contains the full project tree: wgpu-api/, build.rs, ui.slint,
# bluetooth-widget.slint, Cargo.lock, scripts/, assets/, src/, and Cargo.toml.
source=("$pkgname-$pkgver.tar.gz::https://github.com/resdon/dock/archive/refs/tags/$pkgver.tar.gz")
sha256sums=('SKIP')

prepare() {
  cd "$pkgname-$pkgver"
  cargo fetch --locked --target "$(rustc -vV | sed -n 's/host: //p')"
}

build() {
  cd "$pkgname-$pkgver"
  export RUSTUP_TOOLCHAIN=stable
  export CARGO_TARGET_DIR=target
  cargo build --frozen --release --all-features
}

check() {
  cd "$pkgname-$pkgver"
  cargo test --frozen --all-features
}

package() {
  cd "$pkgname-$pkgver"
  # Binary
  install -Dm755 "target/release/dock" "$pkgdir/usr/bin/dock"

  # Shared Assets
  install -Dm755 "scripts/launcher.sh" "$pkgdir/usr/share/dock/launcher.sh"
  install -Dm644 "assets/font.ttf" "$pkgdir/usr/share/dock/font.ttf"

  # Animation Frame SVGs
  install -d "$pkgdir/usr/share/dock/24"
  install -Dm644 assets/24/*.svg -t "$pkgdir/usr/share/dock/24/"

  # Generate System Icon Cache Index
  chmod +x scripts/list_icons.sh
  ./scripts/list_icons.sh "$pkgdir/usr/share/dock/icon_list.txt"
}
