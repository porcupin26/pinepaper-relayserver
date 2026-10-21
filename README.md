# PinePaper Relay Server

[![Build and packages](https://github.com/porcupin26/pinepaper-relayserver/actions/workflows/build.yml/badge.svg)](https://github.com/porcupin26/pinepaper-relayserver/actions/workflows/build.yml)

An ephemeral Rust WebSocket relay for [PinePaper](https://github.com/porcupin26/pinepaper).
The repository is named `pinepaper-relayserver`; the executable, package and
systemd service are named **`pinepaper-exchange`**. Clients create a room or join
with a five-character code such as `1G6TW`. Every participant uses the same server URL.

**The server never saves documents.** It has no database, filesystem storage, document cache, snapshots, operation history, or replay. It maintains live room membership and bounded forwarding queues in RAM, plus short-lived connection-rate counters. Forwarding buffers necessarily contain messages while they are being delivered. Payloads and room codes are not logged. The last disconnect immediately removes the room; restarting the server ends all rooms.

PinePaper clients keep their own local SQLite copies. An existing participant supplies a document to a newcomer. Any participant with the document can do this; the original creator can leave. If everyone leaves, someone must start a new room from their local copy.

## Install a package

Open [Actions → Test and package relay](https://github.com/porcupin26/pinepaper-relayserver/actions/workflows/build.yml),
select a successful run, and download `relay-linux-x86_64-packages` for Intel/AMD
Linux or `relay-linux-aarch64-packages` for ARM64. Extract the artifact ZIP into
its own directory and verify `sha256sum -c SHA256SUMS`. A GitHub login is needed
to download workflow artifacts; they are retained for 30 days.

Install **one** format matching your distribution:

```sh
# Debian / Ubuntu
sudo apt install ./pinepaper-exchange_*.deb
# Fedora / RHEL-family
sudo dnf install ./pinepaper-exchange-*.rpm
# openSUSE
sudo zypper install ./pinepaper-exchange-*.rpm
# Arch / Arch Linux ARM (pacman binary package)
sudo pacman -U ./pinepaper-exchange-*.pkg.tar.zst
```

These packages install `/usr/bin/pinepaper-exchange` and
`/usr/lib/systemd/system/pinepaper-exchange.service`. They do not enable or start
the service automatically. After installation:

```sh
sudo systemctl daemon-reload
sudo systemctl enable --now pinepaper-exchange
curl --fail http://127.0.0.1:8787/healthz
```

The health response is `{"status":"ok","protocol":1,"storage":"none"}` (key order
may vary). The default service listens only on loopback. Put an HTTPS proxy in
front for remote users, as described below. Docs and the example proxy file are
also installed under `/usr/share/doc/pinepaper-exchange/`.

For the portable `.tar.gz`, extract it, enter its `pinepaper-exchange` directory,
and run `sudo bash packaging/install.sh pinepaper-exchange`. This installs to
`/usr/local` with a unit under `/etc/systemd/system`; then run the same systemctl
commands above. Do not mix a source/portable installation with a native package:
a unit in `/etc/systemd/system` takes precedence over the packaged unit.

## Build from source

Requires Linux, Rust 1.96, Cargo and a C toolchain/linker. The relay needs no
OpenSSL development package, desktop environment, display or database.

```sh
git clone https://github.com/porcupin26/pinepaper-relayserver.git
cd pinepaper-relayserver
cargo build --release --locked
```

On Debian/Ubuntu, `sudo apt install build-essential pkg-config` supplies the
system build tools. On Arch use `sudo pacman -S --needed base-devel rust`;
on Fedora use `sudo dnf install gcc gcc-c++ rust cargo`. If the distribution's
Rust version is older than 1.96, install the 1.96 toolchain with rustup.

## Test on your computer

Requires Rust 1.96 (the version used by CI).

```sh
cargo test --locked
cargo run --release --locked
```

The default address is `127.0.0.1:8787`. Set PinePaper **Settings → Sharing → Exchange server** to `http://127.0.0.1:8787` on this computer.

For a tablet on your trusted local network:

```sh
PINEPAPER_EXCHANGE_BIND=0.0.0.0:8787 cargo run --release --locked
```

Use `http://YOUR_COMPUTER_LAN_IP:8787` on both devices, allow TCP port 8787 through your local firewall, and keep the computer awake. Plain HTTP is suitable for this local test; use HTTPS for your prepared server.

In PinePaper, open a document and choose **Page menu → Share document → Share current document**. Give the code to another user. They choose **Create → Join shared document**, enter the code, and receive an editable local copy. Use **Sharing → Leave sharing** to leave. Changing the configured URL affects the next session, not an already connected room.

## Your prepared server

### Native service behind HTTPS

For a source installation (native packages already include the service):

```sh
cargo build --release --locked
sudo bash packaging/install.sh target/release/pinepaper-exchange
sudo systemctl daemon-reload
sudo systemctl enable --now pinepaper-exchange
curl http://127.0.0.1:8787/healthz
```

Install Caddy, adapt `deploy/Caddyfile` (or `/usr/share/doc/pinepaper-exchange/examples/Caddyfile` for packages) to your hostname, point DNS at the server, and allow ports 80/443. Merge the example into your Caddy configuration, then validate and reload Caddy. The relay stays on loopback; Caddy terminates TLS and supports the WebSocket upgrade. Configure PinePaper with `https://exchange.your-domain.example`.

The service uses a dynamic unprivileged user and a read-only filesystem. It requires no writable document directory, volume or database. To change the listen address, use a systemd drop-in with `Environment=PINEPAPER_EXCHANGE_BIND=127.0.0.1:PORT`.

See [Caddy's reverse proxy documentation](https://caddyserver.com/docs/caddyfile/directives/reverse_proxy) for TLS, proxy and timeout configuration. Avoid short WebSocket connection timeouts; server pings keep idle sessions alive. If you mount the relay under a URL prefix, strip that prefix in your proxy and include it in the configured client URL.

### Container

From a source checkout (including `Dockerfile`, `Cargo.lock` and `src/`):

```sh
docker compose -f deploy/compose.yaml up --build -d
curl http://127.0.0.1:8787/healthz
```

The included Compose file binds only to host loopback and uses a read-only container without volumes. Place an HTTPS proxy in front. Its image contains only the executable and runtime libraries. No document persistence is added by the container. Docker Compose builds for the host architecture (x86_64 or ARM64).

CI also provides a `relay-container-amd64` artifact. Extract it and use `docker load -i pinepaper-exchange-container-amd64.tar.gz`; the loaded image is `pinepaper-exchange:ci`. It is an image archive, not a published container registry tag.

## Configuration, operations and troubleshooting

`PINEPAPER_EXCHANGE_BIND` is the only server configuration variable. Examples:
`127.0.0.1:8787` (default), `0.0.0.0:8787` (all IPv4 interfaces), or `[::]:8787`
(IPv6). There is no config file containing documents or credentials. Change
systemd settings with `sudo systemctl edit pinepaper-exchange`:

```ini
[Service]
Environment=PINEPAPER_EXCHANGE_BIND=0.0.0.0:8787
```

Then run `sudo systemctl daemon-reload && sudo systemctl restart pinepaper-exchange`.
Binding all interfaces is useful on a trusted LAN; for HTTPS deployments, keep
loopback binding and expose only the reverse proxy. Use one relay process per
server URL. Independent replicas do not share room membership, so ordinary
round-robin load balancing across replicas will break room joins.

- **Connection refused:** inspect `systemctl status pinepaper-exchange` and
  `journalctl -u pinepaper-exchange -n 50`. Check the bind address and TCP firewall.
- **A tablet cannot connect to localhost:** `localhost` means that tablet. Use
  the server computer's LAN IP or HTTPS domain on every client.
- **HTTP works but sharing does not:** the proxy must allow WebSocket upgrades
  at `/v1/exchange`; check its connection timeouts. `GET /healthz` alone tests HTTP.
- **Room not found:** check the server URL and code; someone with the document
  must still be connected. A server restart or the last disconnect ends the room.
- **TLS failure:** use a valid certificate and matching hostname. PinePaper
  verifies certificates; it has no insecure certificate-bypass switch.
- **Too many attempts:** the relay limits connections by source IP. Behind a
  proxy the relay sees the proxy's IP; see limits below.

For upgrades, install the new package, reload systemd, then restart the service.
Restarting ends active rooms; participants retain local copies and can create
new codes. To stop: `sudo systemctl disable --now pinepaper-exchange`. Remove a
native package with `apt remove pinepaper-exchange`, `dnf remove pinepaper-exchange`,
`zypper remove pinepaper-exchange`, or `pacman -R pinepaper-exchange`, then run
`sudo systemctl daemon-reload`. Source/portable installs can be removed by
stopping the service and deleting their `/usr/local/bin/pinepaper-exchange`,
`/etc/systemd/system/pinepaper-exchange.service`, and installed documentation.

## GitHub Actions and local packaging

Every push, pull request, version tag (`v*`) and manual workflow run produces:

| Package | x86_64 | ARM64 |
| --- | --- | --- |
| Arch/pacman `.pkg.tar.zst` | Yes | Yes |
| Debian/Ubuntu `.deb` | Yes | Yes |
| Fedora/openSUSE `.rpm` | Yes | Yes |
| Portable `.tar.gz` + installer | Yes | Yes |
| Docker image archive | Yes | Build locally using Docker/Compose |

Native packages use Ubuntu 22.04 runners and declare the executable's minimum
glibc version. CI runs rustfmt, Clippy and relay tests on both architectures,
then extracts every native package to verify its binary, service path and docs.
The Docker job verifies HTTP health, absence of filesystem changes, and clean
SIGTERM shutdown in a read-only, unprivileged container. Packages, image archives
and SHA-256 manifests are attached to the workflow run; nothing is automatically
published to a package repository, GitHub Release or container registry.

To build native packages locally, install Python 3, binutils, curl, tar and zstd,
then use the checksum-pinned nFPM installer (2.48.0):

```sh
bash packaging/install-build-tools.sh .tools
export PATH="$PWD/.tools:$PATH"
cargo build --release --locked
bash packaging/build-packages.sh target/release/pinepaper-exchange "$(uname -m)"
# Requires dpkg-deb, rpm/rpm2cpio and cpio in addition to tar/zstd:
bash packaging/verify-packages.sh target/release/pinepaper-exchange "$(uname -m)"
```

Output goes to `dist/`. Native ARM64 builds run directly on ARM64 Linux. To
cross-compile on x86 Linux, install `gcc-aarch64-linux-gnu`, add the Rust target
with `rustup target add aarch64-unknown-linux-gnu`, then:

```sh
CARGO_TARGET_AARCH64_UNKNOWN_LINUX_GNU_LINKER=aarch64-linux-gnu-gcc \
  cargo build --release --locked --target aarch64-unknown-linux-gnu
bash packaging/build-packages.sh target/aarch64-unknown-linux-gnu/release/pinepaper-exchange aarch64
```

`packaging/PKGBUILD` is an optional Arch source build; it expects the matching
`v0.1.0` source tag. Before that tag exists, use Cargo/nFPM as above. This server
is a background service, so desktop formats such as AppImage and Flatpak are
provided by the PinePaper app repository instead.

## Access and limits

- A room code grants full editing access. There are no accounts, passwords, read-only roles or end-to-end encryption. TLS protects the connection; the relay operator could inspect transient traffic. A five-character code is a convenience invitation, not strong authentication. Keep a public deployment private to your group through a VPN or network access controls if that is required.
- 256 simultaneous connections, 256 rooms, 32 participants per room; 128 KiB WebSocket message/frame limit; 4 MiB/s incoming per connection; bounded output queues. Slow or oversized senders/recipients disconnect instead of silently dropping edits.
- 60 connection attempts per minute per source IP. Behind a reverse proxy this limit is shared by all connections from the proxy. The relay deliberately does not trust forwarded IP headers; add per-client rate limits at the proxy if needed.
- Clients reconnect while another participant remains. If the room has ended, local files remain available and must be shared with a new code. App restarts also require a new sharing session. No offline mailbox or server recovery exists.
- PinePaper shares notebooks, paginated books, and PDFs with annotations. PDF originals transfer directly between clients through the relay (maximum 64 MiB); packet limit is 128 MiB after decompression. Client CRDT state is limited to 2,048 pages and 200,000 fields/objects including deletion markers.
- Concurrent additions merge by unique object IDs. Concurrent edits to the same object or metadata field resolve deterministically by Lamport clock/actor. Undo affects your own changes and does not revert a later peer edit. This is object-level collaboration, not character-level collaborative typing.

## Protocol and verification

See [PROTOCOL.md](PROTOCOL.md). `cargo test --locked` runs real WebSocket clients against the relay, checking room isolation, directed/broadcast delivery, no history replay, departure of the creator, room expiry, invalid messages and frame limits.

To run PinePaper's real three-client integration test, start this relay, then from the separate PinePaper repository:

```sh
PINEPAPER_EXCHANGE_TEST_URL=http://127.0.0.1:8787 \
  cargo test --locked live_three_clients -- --ignored --nocapture
```

That test uses temporary client libraries and checks simultaneous ink, collaborative undo/redo, page changes, deferred updates during text editing, reconnection, joining after the creator leaves, and persistence after the room ends. It does not use your personal documents.

## Repository contents and license

`src/` contains the relay; `tests/` exercises the real WebSocket protocol;
`deploy/` contains systemd, Caddy and Compose examples; `packaging/` creates native
installers; `.github/workflows/build.yml` defines CI. `Cargo.lock` is committed.
Builds, packages, environment files, private keys and local databases are ignored.
The source is MIT licensed; see [LICENSE](LICENSE).
