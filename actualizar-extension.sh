#!/usr/bin/env bash
#
# Empaqueta la extensión de VS Code y la instala, para probar un cambio sin
# pasar por el Marketplace.
#
#   ./actualizar-extension.sh                  # empaqueta e instala
#   ./actualizar-extension.sh --bump           # sube la versión de parche
#   ./actualizar-extension.sh --solo-empaquetar
#
# No compila Orion: el compilador ya no va dentro del paquete, se descarga de
# la última release.
set -euo pipefail

REPO="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
EXT="${ORION_EXT_DIR:-$(cd "$REPO/.." && pwd)/orion-extension}"

[[ -d "$EXT" ]] || {
  echo "ERROR: no existe la carpeta de la extensión: $EXT"
  echo "  (define ORION_EXT_DIR=/ruta/a/orion-extension si está en otro sitio)"
  exit 1
}

BUMP=no
SOLO_EMPAQUETAR=no
for arg in "$@"; do
  case "$arg" in
    --bump)            BUMP=yes ;;
    --solo-empaquetar) SOLO_EMPAQUETAR=yes ;;
    --skip-build)      ;;   # aceptado y sin efecto: ya no se compila nada
    *) echo "Opción desconocida: $arg"; exit 1 ;;
  esac
done

cd "$EXT"

# Por defecto no se toca la versión: --force reinstala la misma, y subirla en
# cada prueba la deja por delante del Marketplace sin nada que contar.
if [[ "$BUMP" == "yes" ]]; then
  anterior="$(node -p "require('./package.json').version")"
  node -e '
    const fs = require("fs");
    const p = JSON.parse(fs.readFileSync("package.json", "utf8"));
    const [ma, mi, pa] = p.version.split(".").map(Number);
    p.version = `${ma}.${mi}.${pa + 1}`;
    fs.writeFileSync("package.json", JSON.stringify(p, null, 2) + "\n");
  '
  echo "==> Versión $anterior -> $(node -p "require('./package.json').version")"
fi

VERSION="$(node -p "require('./package.json').version")"
NOMBRE="$(node -p "require('./package.json').name")"
VSIX="$EXT/$NOMBRE-$VERSION.vsix"

# El nombre sale de `name` del package.json: fijarlo a mano es lo que rompió
# este script en el rename a `oriondev`.
echo "==> Empaquetando $NOMBRE $VERSION..."
if [[ -x ./node_modules/.bin/vsce ]]; then
  ./node_modules/.bin/vsce package
else
  npx --yes @vscode/vsce package
fi

[[ -f "$VSIX" ]] || { echo "ERROR: no se generó $VSIX"; exit 1; }
echo "==> $(du -h "$VSIX" | cut -f1)  $VSIX"

[[ "$SOLO_EMPAQUETAR" == "yes" ]] && { echo "LISTO (sin instalar)."; exit 0; }

CODE="code"
command -v code >/dev/null 2>&1 || CODE="/c/Users/lenovo/AppData/Local/Programs/Microsoft VS Code/bin/code"
echo "==> Instalando en VS Code..."
"$CODE" --install-extension "$VSIX" --force

echo ""
echo "LISTO: $NOMBRE $VERSION instalada. Recarga la ventana (Ctrl+Shift+P -> Reload Window)."

BIN_LOCAL="$REPO/orion-vm/target/release/orion.exe"
if [[ -f "$BIN_LOCAL" ]]; then
  echo "Para usar tu compilador local en vez del de la release, añade a tus ajustes:"
  echo "  \"orion.executablePath\": \"${BIN_LOCAL//\\//}\""
fi
