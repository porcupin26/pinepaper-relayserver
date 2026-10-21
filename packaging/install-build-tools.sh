#!/usr/bin/env bash
set -euo pipefail
destination=${1:-"$PWD/.tools"}
mkdir -p "$destination"
arch=$(uname -m)
case "$arch" in
  x86_64) nfpm_arch=x86_64; nfpm_sha=6d7f9a28b2f397934403950fcbba83f598b08d02edbe79a0815b3b4897341750;;
  aarch64) nfpm_arch=arm64; nfpm_sha=a9a75213763783b7669719e505474c748c5ad7dc66591a195619deb50f85c2f4;;
  *) exit 1;;
esac
curl -fL --retry 3 "https://github.com/goreleaser/nfpm/releases/download/v2.48.0/nfpm_2.48.0_Linux_${nfpm_arch}.tar.gz" -o "$destination/nfpm.tar.gz"
printf '%s  %s\n' "$nfpm_sha" "$destination/nfpm.tar.gz" | sha256sum -c -
tar -xzf "$destination/nfpm.tar.gz" -C "$destination" nfpm
chmod 755 "$destination/nfpm"
