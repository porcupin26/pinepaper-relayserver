#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")/.."
export PACKAGE_BINARY=${1:-target/release/pinepaper-exchange}
arch=${2:-$(uname -m)}
case "$arch" in
  x86_64) export PACKAGE_ARCH=amd64;;
  aarch64) export PACKAGE_ARCH=arm64;;
  *) printf 'Unsupported architecture: %s\n' "$arch" >&2; exit 1;;
esac
machine=$(readelf -h "$PACKAGE_BINARY" | sed -n 's/^ *Machine: *//p')
case "$arch:$machine" in
  x86_64:*X86-64*|aarch64:AArch64) ;;
  *) printf 'Binary architecture does not match %s: %s\n' "$arch" "$machine" >&2; exit 1;;
esac
export PACKAGE_VERSION PACKAGE_LIBC_MIN PACKAGE_SERVICE
PACKAGE_VERSION=$(sed -n 's/^version = "\(.*\)"/\1/p' Cargo.toml | head -1)
PACKAGE_LIBC_MIN=$(readelf --version-info "$PACKAGE_BINARY" | sed -n 's/.*Name: GLIBC_\([0-9.]*\) .*/\1/p' | sort -V | tail -1)
test -n "$PACKAGE_LIBC_MIN"
mkdir -p dist
stage=$(mktemp -d)
trap 'rm -rf "$stage"' EXIT
PACKAGE_SERVICE="$stage/pinepaper-exchange.service"
sed 's|/usr/local/bin/pinepaper-exchange|/usr/bin/pinepaper-exchange|' deploy/pinepaper-exchange.service > "$PACKAGE_SERVICE"
python3 - "$stage/nfpm.yaml" <<'PY'
import datetime, os, pathlib, sys, time
text = pathlib.Path('packaging/nfpm.yaml').read_text()
epoch = int(os.environ.get('SOURCE_DATE_EPOCH', time.time()))
mtime = datetime.datetime.fromtimestamp(epoch, datetime.timezone.utc).strftime('%Y-%m-%dT%H:%M:%SZ')
text = text.replace('${PACKAGE_MTIME}', mtime)
for key in ('PACKAGE_BINARY','PACKAGE_ARCH','PACKAGE_VERSION','PACKAGE_LIBC_MIN','PACKAGE_SERVICE'):
    text = text.replace('${' + key + '}', os.environ[key])
pathlib.Path(sys.argv[1]).write_text(text)
PY
for format in deb rpm archlinux; do
  nfpm package --config "$stage/nfpm.yaml" --packager "$format" --target dist/
done
# The portable archive installs to /usr/local and carries its own installer.
portable="$stage/pinepaper-exchange"
mkdir -p "$portable/deploy" "$portable/packaging"
cp "$PACKAGE_BINARY" "$portable/pinepaper-exchange"
cp README.md PROTOCOL.md LICENSE "$portable/"
cp deploy/pinepaper-exchange.service deploy/Caddyfile "$portable/deploy/"
cp packaging/install.sh "$portable/packaging/"
tar -C "$stage" -czf "dist/pinepaper-exchange-${PACKAGE_VERSION}-linux-${arch}.tar.gz" pinepaper-exchange
printf 'Built DEB, RPM, Arch and tar packages for %s\n' "$arch"
