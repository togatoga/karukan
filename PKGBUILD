# Maintainer: Your Name <your@email.com>

pkgbase=karukan
pkgname=('fcitx5-karukan' 'karukan-cli')
pkgver=0.1.0
pkgrel=1
pkgdesc='Japanese Input Method system with neural kana-kanji conversion'
arch=('x86_64')
url='https://github.com/togatoga/karukan'
license=('MIT' 'Apache-2.0')
makedepends=(
  'rust'
  'cargo'
  'cmake'
  'extra-cmake-modules'
  'fcitx5'
  'pkg-config'
  'libxkbcommon'
  'openssl'
  'gcc'
)

build() {
  cd "$startdir"

  # Save makepkg flags (they interfere with llama-cpp-sys-2 / onig_sys bundled builds)
  local _saved_cflags="$CFLAGS"
  local _saved_cxxflags="$CXXFLAGS"
  local _saved_ldflags="$LDFLAGS"
  unset CFLAGS CXXFLAGS LDFLAGS

  # Clear CMAKE_ variables (llama-cpp-sys-2 build.rs passes them all to CMake)
  for _var in $(env | grep -o '^CMAKE_[^=]*'); do
    unset "$_var"
  done

  # Clean stale build artifacts (makepkg env vars corrupt native lib builds
  # and cargo doesn't invalidate cache when CFLAGS/CMAKE_* change)
  cargo clean --release

  # Build all Rust crates
  cargo build --release

  # Build fcitx5 addon (keep makepkg flags unset because cmake's custom
  # target re-runs cargo, which would corrupt native libs with CFLAGS)
  cd karukan-im/fcitx5-addon
  cmake -B build -DCMAKE_INSTALL_PREFIX=/usr
  cmake --build build -j
}

package_fcitx5-karukan() {
  pkgdesc='Fcitx5 addon for karukan Japanese Input Method'
  depends=('fcitx5' 'libxkbcommon')

  cd "$startdir/karukan-im/fcitx5-addon"
  DESTDIR="$pkgdir" cmake --install build
}

package_karukan-cli() {
  pkgdesc='CLI tools for karukan Japanese Input Method (server, dict builder, bench)'

  cd "$startdir"
  install -Dm755 target/release/karukan-server "$pkgdir/usr/bin/karukan-server"
  install -Dm755 target/release/karukan-dict "$pkgdir/usr/bin/karukan-dict"
  install -Dm755 target/release/sudachi-dict "$pkgdir/usr/bin/sudachi-dict"
  install -Dm755 target/release/ajimee-bench "$pkgdir/usr/bin/ajimee-bench"
}
