# Maintainer: Nekoda-jun
pkgname=aurore-player
pkgver=0.1.0
pkgrel=1
pkgdesc="Audio-only player for Linux, WMA via ffmpeg (Rust, iced + rodio)"
arch=('x86_64')
url="https://github.com/ALBAlinux-project/Aurore-player"
license=('GPL-3.0-or-later')
depends=('gtk3' 'alsa-lib' 'ffmpeg')
makedepends=('cargo' 'git')
source=("$pkgname::git+$url.git")
sha256sums=('SKIP')

build() {
    cd "$srcdir/$pkgname"
    cargo build --release --locked
}

package() {
    cd "$srcdir/$pkgname"
    install -Dm755 target/release/aurore-player "$pkgdir/usr/bin/aurore-player"
    install -Dm644 LICENSE "$pkgdir/usr/share/licenses/$pkgname/LICENSE"
}
