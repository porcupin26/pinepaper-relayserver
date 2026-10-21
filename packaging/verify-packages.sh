#!/usr/bin/env bash
# Inspect each native package and compare the installed executable to the build.
set -euo pipefail
cd "$(dirname "$0")/.."
binary=${1:-target/release/pinepaper-exchange}
arch=${2:-$(uname -m)}
case "$arch" in x86_64) debarch=amd64;; aarch64) debarch=arm64;; *) exit 1;; esac
version=$(sed -n 's/^version = "\(.*\)"/\1/p' Cargo.toml | head -1)
stage=$(mktemp -d)
trap 'rm -rf "$stage"' EXIT
mkdir -p "$stage/arch" "$stage/rpm" "$stage/tar" "$stage/rpmdb"
deb="dist/pinepaper-exchange_${version}-1_${debarch}.deb"
rpm="dist/pinepaper-exchange-${version}-1.${arch}.rpm"
archive="dist/pinepaper-exchange-${version}-1-${arch}.pkg.tar.zst"
dpkg-deb --info "$deb"
rpm --dbpath "$stage/rpmdb" -qpi "$rpm"
dpkg-deb -x "$deb" "$stage/deb"
tar --zstd -xf "$archive" -C "$stage/arch"
rpm2cpio "$rpm" | (cd "$stage/rpm" && cpio -idm --quiet --no-absolute-filenames)
for format in deb rpm arch; do
  cmp "$binary" "$stage/$format/usr/bin/pinepaper-exchange"
  test -x "$stage/$format/usr/bin/pinepaper-exchange"
  grep -Fxq 'ExecStart=/usr/bin/pinepaper-exchange' "$stage/$format/usr/lib/systemd/system/pinepaper-exchange.service"
  test -s "$stage/$format/usr/share/doc/pinepaper-exchange/README.md"
done
tar -xf "dist/pinepaper-exchange-${version}-linux-${arch}.tar.gz" -C "$stage/tar"
portable="$stage/tar/pinepaper-exchange"
(cd "$portable" && DESTDIR="$stage/install" PREFIX=/usr/local bash packaging/install.sh pinepaper-exchange)
cmp "$binary" "$stage/install/usr/local/bin/pinepaper-exchange"
grep -Fxq 'ExecStart=/usr/local/bin/pinepaper-exchange' "$stage/install/etc/systemd/system/pinepaper-exchange.service"
printf 'All four package formats contain the expected binary, service and documentation.\n'
