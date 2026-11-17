#!/bin/sh
# Installs the latest dedalo binary for this machine into ~/.local/bin.
#
#     curl -fsSL https://github.com/FreshMag/dedalo/releases/latest/download/install.sh | sh
#
# DEDALO_REPO (owner/name) and DEDALO_INSTALL_DIR change the defaults.
set -eu
repo="${DEDALO_REPO:-FreshMag/dedalo}"
dir="${DEDALO_INSTALL_DIR:-$HOME/.local/bin}"
case "$(uname -s)-$(uname -m)" in
  Linux-x86_64) target=x86_64-unknown-linux-musl ;;
  Linux-aarch64 | Linux-arm64) target=aarch64-unknown-linux-musl ;;
  Darwin-arm64) target=aarch64-apple-darwin ;;
  Darwin-x86_64) target=x86_64-apple-darwin ;;
  *) echo "no prebuilt dedalo for $(uname -s) $(uname -m); use: cargo install dedalo-cli" >&2; exit 1 ;;
esac
base="https://github.com/$repo/releases/latest/download"
tmp="$(mktemp -d)"
trap 'rm -rf "$tmp"' EXIT
curl -fsSL "$base/dedalo-$target.tar.gz" -o "$tmp/dedalo.tar.gz"
curl -fsSL "$base/SHA256SUMS" -o "$tmp/SHA256SUMS"
expected="$(grep " dedalo-$target.tar.gz\$" "$tmp/SHA256SUMS" | cut -d' ' -f1)"
actual="$( (sha256sum "$tmp/dedalo.tar.gz" 2>/dev/null || shasum -a 256 "$tmp/dedalo.tar.gz") | cut -d' ' -f1)"
[ "$expected" = "$actual" ] || { echo "checksum mismatch; not installing" >&2; exit 1; }
tar xzf "$tmp/dedalo.tar.gz" -C "$tmp"
mkdir -p "$dir"
cp "$tmp/dedalo-$target/dedalo" "$dir/dedalo"
chmod +x "$dir/dedalo"
echo "installed $("$dir/dedalo" --version) to $dir"
