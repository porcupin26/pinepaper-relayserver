#!/usr/bin/env bash
# Install into a real prefix or a staging tree for native packages.
set -euo pipefail
binary=${1:-target/release/pinepaper-exchange}
prefix=${PREFIX:-/usr/local}
dest=${DESTDIR:-}
unitdir=${UNITDIR:-/etc/systemd/system}
install -Dm755 "$binary" "$dest$prefix/bin/pinepaper-exchange"
install -Dm644 LICENSE "$dest$prefix/share/licenses/pinepaper-exchange/LICENSE"
for doc in README.md PROTOCOL.md; do
  install -Dm644 "$doc" "$dest$prefix/share/doc/pinepaper-exchange/$doc"
done
install -Dm644 deploy/Caddyfile "$dest$prefix/share/doc/pinepaper-exchange/examples/Caddyfile"
install -Dm644 deploy/pinepaper-exchange.service "$dest$unitdir/pinepaper-exchange.service"
# Source installs use /usr/local; distribution packages use /usr.
sed -i "s|^ExecStart=.*|ExecStart=$prefix/bin/pinepaper-exchange|" "$dest$unitdir/pinepaper-exchange.service"
