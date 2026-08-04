# Paquetes de Orion

Referencia del gestor de paquetes: dónde vive cada cosa, qué formato tienen los
archivos y cómo se instala desde fuera del registry oficial.

## Dónde se buscan los paquetes

Todo lo resuelve `orion-vm/src/paths.rs`; el gestor, el `use` del runtime y
`orion doctor` comparten esas rutas. Si alguna vez discrepan, es un fallo.

La **raíz del proyecto** es el ancestro más cercano al archivo que se ejecuta
que contenga `orion.json`. Si no hay manifiesto, vale un ancestro que contenga
`packages/`, y en último término el directorio actual.

Que la raíz dependa del archivo y no del directorio de invocación es la
diferencia importante: `orion run src/hondo/main.orx` encuentra los mismos
paquetes lo lances desde donde lo lances.

Orden de búsqueda de un `use "x"`:

1. `<raíz>/packages/x.orx`
2. `<raíz>/x.orx`
3. `<raíz>/lib/x.orx`
4. lo mismo relativo al directorio del archivo de entrada y al directorio actual
5. `<globales>/x.orx`

Los globales son `$ORION_PKGS`, o `$ORION_HOME/packages`, o `~/.orion/packages`.

`orion --add` escribe en el proyecto cuando hay manifiesto o ya existe un
`packages/`; en caso contrario, en el directorio global.

Comprueba en cualquier momento qué está usando tu máquina:

```
orion doctor
```

## orion.json

Sirve a la vez de manifiesto de proyecto y de manifiesto de publicación.

```json
{
  "name": "mi-proyecto",
  "version": "1.0.0",
  "description": "Qué hace",
  "author": "tu-nombre",
  "license": "MIT",
  "file": "mi-proyecto.orx",
  "tags": ["cli", "datos"],

  "dependencies": {
    "http":   "^1.0.0",
    "colors": "*",
    "propia": "gh:usuario/repo:src/propia.orx"
  }
}
```

`dependencies` acepta como valor un especificador de versión o una fuente:

| Especificador | Significa |
|---|---|
| `1.2.3` | exactamente esa versión |
| `^1.2.3` | mismo major, igual o superior |
| `~1.2.3` | mismo major.minor, igual o superior |
| `>=1.2.3`, `>`, `<=`, `<` | comparación directa |
| `*`, `latest` | cualquiera |
| `https://…/x.orx` | URL directa |
| `gh:owner/repo[@ref][:ruta]` | repositorio de GitHub |
| `./ruta/x.orx` | archivo local |

```
orion install
```

instala las dependencias declaradas y escribe `orion.lock`.

## orion.lock

Se genera solo. Fija qué se instaló y con qué contenido exacto, de modo que
reinstalar el proyecto en otra máquina dé lo mismo:

```json
{
  "lockfileVersion": 1,
  "packages": {
    "http": {
      "version":  "1.0.0",
      "resolved": "https://raw.githubusercontent.com/…/http.orx",
      "sha256":   "9f2c…",
      "source":   "builtin"
    }
  }
}
```

Cuando existe, `orion install` exige que lo descargado tenga ese `sha256` y
aborta si no coincide. Versiónalo con el repositorio.

## Instalar desde fuera del registry

El registry oficial ya no es la única puerta:

```
orion add colors                                  # registry
orion add https://ejemplo.dev/paquete.orx         # URL directa
orion add gh:usuario/repo                         # repo de GitHub
orion add gh:usuario/repo@v2:src/lib.orx          # rama/tag y ruta concretos
orion add ./libs/util.orx                         # archivo local
```

Con `--sha256 <hex>` se fija el contenido aceptado, que es como se instala desde
un origen arbitrario sin tener que confiar en el servidor:

```
orion add https://ejemplo.dev/paquete.orx --sha256 9f2c…
```

Sin `--sha256`, el hash de lo recibido se calcula igual y se anota en
`installed.json`, de modo que un cambio silencioso posterior es detectable.

Para apuntar a otro registry entero (uno propio, o un espejo interno):

```
ORION_REGISTRY=https://registry.miempresa.dev/orion
```

## registry.json

```json
{
  "_meta": { "registry": "https://…/packages" },
  "packages": {
    "browser": {
      "version":     "0.1.0",
      "description": "Automatización de navegador vía CDP",
      "author":      "orion-core",
      "type":        "native",
      "file":        "browser.orx",
      "sha256":      "<sha256 del .orx>",
      "tags":        ["web", "scraping"],

      "dependencies": { "http": "^1.0.0" },

      "assets": {
        "win32-x64":    { "url": "https://…/browser-win32-x64.dll",   "sha256": "…" },
        "linux-x64":    { "url": "https://…/libbrowser-linux-x64.so", "sha256": "…" },
        "darwin-arm64": { "url": "https://…/libbrowser-darwin.dylib", "sha256": "…",
                          "signature": "<firma base64>" }
      }
    }
  }
}
```

`sha256`, `dependencies` y `assets` son opcionales: una entrada del esquema
antiguo sigue cargando sin cambios.

## Paquetes con código nativo

`assets` es lo que permite que un paquete traiga una librería dinámica en vez de
solo Orion. Al instalar se descarga el asset de la plataforma actual
(`win32-x64`, `linux-x64`, `darwin-arm64`) y queda en:

```
<packages>/native/<paquete>/<librería>
```

de donde la recoge `extern … from "<lib>"` sin que haya que tocar el `PATH` ni
`LD_LIBRARY_PATH`.

Dos reglas deliberadas:

- **Un asset sin `sha256` no se instala.** Es el único punto del flujo en el que
  se ejecuta código que no compiló el usuario, y ahí no hay margen para
  confiar en que la descarga llegó entera.
- **La firma es opcional y verifica autoría, no integridad.** El `sha256` ya
  garantiza que el binario es exactamente el que el registry declara; la firma
  añade *quién* lo declara, y eso solo significa algo si tú decides en quién
  confías.

Las claves de confianza son archivos `.pem` con claves públicas RSA en
`<globales>/trusted_keys/`, o donde apunte `ORION_TRUSTED_KEYS`. Sin claves
instaladas, un asset firmado se instala igual tras verificar el checksum y se
avisa de que la firma no se pudo comprobar.

## Publicar

```
$env:ORION_GITHUB_TOKEN = "<token>"
orion publish
```

Sube el `.orx`, añade la entrada al `registry.json` con su `sha256` calculado y
abre un Pull Request. Si `orion.json` declara `dependencies` o `assets`, se
copian a la entrada del registry.
