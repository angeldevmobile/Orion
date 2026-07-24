#!/usr/bin/env bash
#
# Reempaqueta e instala la extensión VSCode de Orion con el binario y el
# server.js actuales del repo. Automatiza los 4 pasos manuales:
#   1) compilar Orion en release   2) copiar el binario al bundle
#   3) subir la versión de parche   4) empaquetar (.vsix) e instalar
#
# Uso:
#   ./actualizar-extension.sh              # compila release y luego empaqueta
#   ./actualizar-extension.sh --skip-build # usa el binario release ya compilado
#
set -euo pipefail

# REPO = repo del lenguaje (donde vive este script y el crate orion-vm).
# EXT  = repo de la extensión, ahora en una carpeta HERMANA (orion-extension),
#        tras la reorganización 2026-07-23 (antes: vscode-orion/orion-lang).
REPO="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
EXT="${ORION_EXT_DIR:-$(cd "$REPO/.." && pwd)/orion-extension}"
BIN_SRC="$REPO/orion-vm/target/release/orion.exe"
BIN_DST="$EXT/bin/win32-x64/orion.exe"

[[ -d "$EXT" ]] || { echo "ERROR: no existe la carpeta de la extensión: $EXT"; echo "  (define ORION_EXT_DIR=/ruta/a/orion-extension si está en otro sitio)"; exit 1; }
mkdir -p "$(dirname "$BIN_DST")"

# 1) Compilar release (salvo --skip-build)
if [[ "${1:-}" != "--skip-build" ]]; then
  echo "==> Compilando Orion en release (puede tardar varios minutos)..."
  ( cd "$REPO/orion-vm" && cargo build --release )
fi

[[ -f "$BIN_SRC" ]] || { echo "ERROR: no existe $BIN_SRC — compila primero (sin --skip-build)"; exit 1; }

# 2) Copiar el binario release al bundle de la extensión
echo "==> Copiando binario al bundle..."
cp "$BIN_SRC" "$BIN_DST"

# 3) Subir la versión de parche (X.Y.Z -> X.Y.Z+1) en package.json
cur="$(grep -m1 '"version"' "$EXT/package.json" | sed -E 's/.*"version" *: *"([0-9.]+)".*/\1/')"
IFS=. read -r MA MI PA <<< "$cur"
new="$MA.$MI.$((PA + 1))"
sed -i -E "0,/\"version\" *: *\"[0-9.]+\"/s//\"version\": \"$new\"/" "$EXT/package.json"
echo "==> Versión $cur -> $new"

# 4) Empaquetar e instalar
echo "==> Empaquetando .vsix..."
( cd "$EXT" && ./node_modules/.bin/vsce package )

VSIX="$EXT/orion-lang-$new.vsix"
CODE="code"
command -v code >/dev/null 2>&1 || CODE="/c/Users/lenovo/AppData/Local/Programs/Microsoft VS Code/bin/code"
echo "==> Instalando en VS Code..."
"$CODE" --install-extension "$VSIX" --force

echo ""
echo "LISTO: orion-lang $new instalada."
echo "Recarga la ventana de VS Code: Ctrl+Shift+P -> \"Reload Window\"."
