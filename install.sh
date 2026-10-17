#!/usr/bin/env bash
# Lincast Chat one-line installer (Linux x86_64).
#   curl -fsSL https://raw.githubusercontent.com/LinCastr/lincast-chat/main/install.sh | bash
# Extra arguments go to the packaged installer, e.g.:  ... | bash -s -- --prefix /opt/lincast
# Downloads the newest release tarball, verifies its SHA-256 and installs it.
set -euo pipefail

REPO="LinCastr/lincast-chat-releases"

die() { echo "error: $*" >&2; exit 1; }

[[ "$(uname -s)" == "Linux" ]] || die "Linux only"
case "$(uname -m)" in x86_64|amd64) ;; *) die "only x86_64 builds are published";; esac
for cmd in curl tar sha256sum; do command -v "$cmd" >/dev/null || die "missing '$cmd'"; done

tmp="$(mktemp -d)"
trap 'rm -rf "$tmp"' EXIT

echo "Looking for the latest release..."
# The repo also holds old v1 releases, so pick the newest one that ships a
# linux-x86_64 tarball instead of trusting GitHub's "latest" flag.
urls="$(curl -fsSL -H 'Accept: application/vnd.github+json' \
  "https://api.github.com/repos/$REPO/releases?per_page=20" \
  | grep -o '"browser_download_url": *"[^"]*lincast-chat-[^"]*linux-x86_64\.tar\.gz"' \
  | sed 's/.*"\(https[^"]*\)"/\1/')" || die "could not reach GitHub"
tarball_url="$(printf '%s\n' "$urls" | head -n1)"
[[ -n "$tarball_url" ]] || die "no release with a Linux x86_64 package found"
name="${tarball_url##*/}"

echo "Downloading $name..."
curl -fL --progress-bar -o "$tmp/$name" "$tarball_url"
curl -fsSL -o "$tmp/$name.sha256" "$tarball_url.sha256" || die "missing checksum file"
expected="$(awk '{print $1}' "$tmp/$name.sha256")"
actual="$(sha256sum "$tmp/$name" | awk '{print $1}')"
[[ -n "$expected" && "$expected" == "$actual" ]] || die "checksum mismatch, aborting"

tar -xzf "$tmp/$name" -C "$tmp"
dir="$(find "$tmp" -maxdepth 1 -type d -name 'lincast-chat-*' | head -n1)"
[[ -x "$dir/install.sh" ]] || die "package has no installer"
"$dir/install.sh" "$@"

cat <<'MSG'

Done. Start "Lincast Chat" from your app launcher, or run: lincast-chat
Needs PipeWire and an xdg-desktop-portal with ScreenCast support (e.g. xdg-desktop-portal-hyprland/-kde/-gnome).
The app updates itself when a new release is published.
MSG
