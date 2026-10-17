#!/usr/bin/env bash
# scripts/make-release.sh — builds dist/lincast-chat-<version>-linux-x86_64.tar.gz
# from the release binaries (run `cargo build --release` in v2/ first).
set -euo pipefail
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
VERSION="$(sed -n 's/^version = "\(.*\)"/\1/p' "$ROOT/v2/cliente/Cargo.toml" | head -1)"
NAME="lincast-chat-${VERSION}-linux-x86_64"
STAGE="$ROOT/dist/$NAME"
rm -rf "$STAGE"; mkdir -p "$STAGE"
install -m 755 "$ROOT/v2/target/release/lincast-chat" "$ROOT/v2/target/release/concord-audio" "$STAGE/"
install -m 644 "$ROOT/lincast-chat.desktop" "$ROOT/v2/cliente/assets/icone-256.png" "$STAGE/"
cat > "$STAGE/install.sh" <<'INNER'
#!/usr/bin/env bash
# Installs Lincast Chat for the current user (~/.local) or, with --prefix, elsewhere.
set -euo pipefail
HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PREFIX="${HOME}/.local"
[[ "${1:-}" == "--prefix" ]] && PREFIX="${2:?missing path}"
install -D -m 755 "$HERE/lincast-chat" "$PREFIX/bin/lincast-chat"
install -D -m 755 "$HERE/concord-audio" "$PREFIX/bin/concord-audio"
install -D -m 644 "$HERE/icone-256.png" "$PREFIX/share/icons/hicolor/256x256/apps/lincast-chat.png"
sed "s|^Exec=.*|Exec=$PREFIX/bin/lincast-chat %u|" "$HERE/lincast-chat.desktop" \
  | install -D -m 644 /dev/stdin "$PREFIX/share/applications/lincast-chat.desktop"
echo "Installed to $PREFIX. Close and reopen the app if it was running."
INNER
chmod +x "$STAGE/install.sh"
tar -C "$ROOT/dist" -czf "$ROOT/dist/$NAME.tar.gz" "$NAME"
( cd "$ROOT/dist" && sha256sum "$NAME.tar.gz" > "$NAME.tar.gz.sha256" )
echo "$ROOT/dist/$NAME.tar.gz"
