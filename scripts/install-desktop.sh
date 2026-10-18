#!/usr/bin/env bash
# scripts/install-desktop.sh — Instala o Lincast Chat e atalhos de desktop no sistema
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ROOT_DIR="$(cd "${SCRIPT_DIR}/.." && pwd)"

PREFIX="${HOME}/.local"
DESKTOP_ENTRY="${ROOT_DIR}/lincast-chat.desktop"
ICON_SOURCE="${ROOT_DIR}/v2/cliente/assets/icone-256.png"
BIN_CLIENT="${ROOT_DIR}/v2/target/release/lincast-chat"
BIN_AUDIO="${ROOT_DIR}/v2/target/release/concord-audio"
DO_UNINSTALL=0

usage() {
    cat <<EOF
Uso: $0 [OPÇÕES]

Opções:
  --user            Instala para o usuário atual em ~/.local (padrão)
  --system          Instala para todo o sistema em /usr/local (requer sudo)
  --prefix <PATH>   Especifica um diretório de instalação customizado
  --uninstall       Remove o binário, o atalho desktop e os ícones
  -h, --help        Exibe esta mensagem de ajuda
EOF
    exit 0
}

while [[ $# -gt 0 ]]; do
    case "$1" in
        --user)
            PREFIX="${HOME}/.local"
            shift
            ;;
        --system)
            PREFIX="/usr/local"
            shift
            ;;
        --prefix)
            PREFIX="$2"
            shift 2
            ;;
        --uninstall)
            DO_UNINSTALL=1
            shift
            ;;
        -h|--help)
            usage
            ;;
        *)
            echo "Opção desconhecida: $1"
            usage
            ;;
    esac
done

BINDIR="${PREFIX}/bin"
DATADIR="${PREFIX}/share"
APPDIR="${DATADIR}/applications"
ICONDIR="${DATADIR}/icons/hicolor/256x256/apps"

if [[ "${DO_UNINSTALL}" -eq 1 ]]; then
    echo "==> Desinstalando Lincast Chat de ${PREFIX}..."
    rm -f "${BINDIR}/lincast-chat" "${BINDIR}/concord-audio"
    rm -f "${APPDIR}/lincast-chat.desktop"
    rm -f "${ICONDIR}/lincast-chat.png"

    if command -v update-desktop-database >/dev/null 2>&1; then
        update-desktop-database "${APPDIR}" 2>/dev/null || true
    fi
    if command -v gtk-update-icon-cache >/dev/null 2>&1; then
        gtk-update-icon-cache -f -t "${DATADIR}/icons/hicolor" 2>/dev/null || true
    fi
    echo "✓ Desinstalação concluída com sucesso."
    exit 0
fi

# Compila sempre: o cargo só refaz o que mudou. Antes só compilava quando o
# binário não existia, e instalava em silêncio uma versão velha.
echo "==> Compilando a versão de release..."
cargo build --release --manifest-path "${ROOT_DIR}/v2/Cargo.toml"

echo "==> Instalando Lincast Chat em ${PREFIX}..."
install -d -m 755 "${BINDIR}" "${APPDIR}" "${ICONDIR}"

install -m 755 "${BIN_CLIENT}" "${BINDIR}/lincast-chat"
install -m 755 "${BIN_AUDIO}" "${BINDIR}/concord-audio"
install -m 644 "${DESKTOP_ENTRY}" "${APPDIR}/lincast-chat.desktop"
install -m 644 "${ICON_SOURCE}" "${ICONDIR}/lincast-chat.png"

# Atualiza caches do sistema
if command -v update-desktop-database >/dev/null 2>&1; then
    update-desktop-database "${APPDIR}" 2>/dev/null || true
fi
if command -v gtk-update-icon-cache >/dev/null 2>&1; then
    gtk-update-icon-cache -f -t "${DATADIR}/icons/hicolor" 2>/dev/null || true
fi

echo "✓ Lincast Chat instalado com sucesso!"
echo "  Executável: ${BINDIR}/lincast-chat"
echo "  Áudio auxiliar: ${BINDIR}/concord-audio"
echo "  Lançador: ${APPDIR}/lincast-chat.desktop"
echo "  Ícone: ${ICONDIR}/lincast-chat.png"
