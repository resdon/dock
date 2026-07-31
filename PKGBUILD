# Maintainer: resdon
pkgname=dock
pkgver=0.1.0
pkgrel=1
pkgdesc="A Wayland dock application"
arch=('x86_64')
license=('custom')
depends=('wayland' 'libxkbcommon' 'fontconfig' 'gcc-libs')
makedepends=('rust' 'cargo')
source=('Cargo.toml' 'src' 'font.ttf' 'launcher.sh')
sha256sums=('SKIP' 'SKIP' 'SKIP' 'SKIP')

prepare() {
  cargo fetch --locked --target "$(rustc -vV | sed -n 's/host: //p')"
}

build() {
  export RUSTUP_TOOLCHAIN=stable
  export CARGO_TARGET_DIR=target
  cargo build --frozen --release --all-features
}

check() {
  cargo test --frozen --all-features
}

package() {
  # Binary
  install -Dm755 "target/release/dock" "$pkgdir/usr/bin/dock"
  
  # Shared Assets
  install -Dm755 "launcher.sh" "$pkgdir/usr/share/dock/launcher.sh"
  install -Dm644 "font.ttf" "$pkgdir/usr/share/dock/font.ttf"
}