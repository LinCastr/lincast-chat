#!/usr/bin/env bash
# dist/appimage/build-appimage.sh — Constrói a estrutura do AppDir e gera o AppImage
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ROOT_DIR="$(cd "${SCRIPT_DIR}/../.." && pwd)"
APPDIR="${ROOT_DIR}/build/AppDir"
OUTPUT_DIR="${ROOT_DIR}/dist/output"

echo "==> Compilando binários de release..."
cargo build --release --locked --manifest-path "${ROOT_DIR}/v2/Cargo.toml"

echo "==> Preparando estrutura do AppDir em ${APPDIR}..."
rm -rf "${APPDIR}"
mkdir -p "${APPDIR}/usr/bin" \
         "${APPDIR}/usr/share/applications" \
         "${APPDIR}/usr/share/icons/hicolor/256x256/apps" \
         "${OUTPUT_DIR}"

# Instala binários e metadados na hierarquia
install -m 755 "${ROOT_DIR}/v2/target/release/lincast-chat" "${APPDIR}/usr/bin/lincast-chat"
install -m 755 "${ROOT_DIR}/v2/target/release/concord-audio" "${APPDIR}/usr/bin/concord-audio"
install -m 644 "${ROOT_DIR}/lincast-chat.desktop" "${APPDIR}/usr/share/applications/lincast-chat.desktop"
install -m 644 "${ROOT_DIR}/v2/cliente/assets/icone-256.png" "${APPDIR}/usr/share/icons/hicolor/256x256/apps/lincast-chat.png"

# Arquivos raiz obrigatórios pelo padrão AppImage
install -m 755 "${SCRIPT_DIR}/AppRun" "${APPDIR}/AppRun"
cp "${ROOT_DIR}/lincast-chat.desktop" "${APPDIR}/lincast-chat.desktop"
cp "${ROOT_DIR}/v2/cliente/assets/icone-256.png" "${APPDIR}/lincast-chat.png"

echo "✓ AppDir preparado com sucesso."

# Se o appimagetool estiver instalado, gera o executável AppImage
if command -v appimagetool >/dev/null 2>&1; then
    echo "==> Gerando Lincast-Chat-x86_64.AppImage com appimagetool..."
    ARCH=x86_64 appimagetool "${APPDIR}" "${OUTPUT_DIR}/Lincast-Chat-x86_64.AppImage"
    echo "✓ AppImage gerado em: ${OUTPUT_DIR}/Lincast-Chat-x86_64.AppImage"
else
    echo "ℹ Nota: 'appimagetool' não encontrado no PATH."
    echo "  O diretório ${APPDIR} está pronto para ser empacotado."
fi
