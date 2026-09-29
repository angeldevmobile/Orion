# Paquetes: dónde viven y qué archivos usan

Referencia del gestor de paquetes (`orion --add`, `--remove`, `--list`,
`--search`, `--update`, `--install`, `--publish`). El código está en
`orion-vm/src/pkg.rs` (qué se instala y de dónde) y `orion-vm/src/paths.rs`
(dónde).

## Rutas

`paths.rs` es la única fuente de verdad. El gestor, el `use` del runtime y
`orion doctor` la comparten. Antes cada uno resolvía por su cuenta y no
coincidían: `orion doctor` decía "ningún paquete instalado" con diez paquetes
instalados.

- **Raíz de proyecto**: el ancestro más cercano al archivo que se ejecuta y que
  contenga `orion.json`. Si no hay manifiesto, vale un ancestro con `packages/`,
  y en último término el directorio actual.
- **Paquetes de proyecto**: `<raíz>/packages`, que se pueden versionar con el
  repo.
- **Paquetes globales**: `$ORION_PKGS`, `$ORION_HOME/packages` o
  `~/.orion/packages`, compartidos entre proyectos.

La búsqueda va de lo más específico a lo más general: primero el proyecto,
después lo global. `--add` instala en el proyecto si hay manifiesto o ya existe
un `packages/`, y en el global si no, para no crear un `packages/` en cualquier
directorio.

`use "<ruta>"` prueba, en orden, la raíz del proyecto, el directorio del
archivo de entrada y el directorio actual, con y sin `packages/` delante.

## Archivos

`registry.json`, el índice del registro:

```
{ "_meta": { "registry": "<base_url>", ... },
  "packages": { "<name>": {
      "version", "description", "file", "type", "author", "tags",
      "sha256"?,                       // integridad del .orx
      "dependencies"? { "<pkg>": "<spec>" },
      "assets"? { "<plataforma>": { "url", "sha256", "signature"? } }
  } } }
```

`installed.json`, lo instalado:

```
{ "<name>": { "version", "description", "file", "source", "sha256"?, "native"? } }
```

`orion.json`, el manifiesto de proyecto y de publicación:

```
{ "name", "version", "description", "author", "tags", "file", "license",
  "dependencies"? { "<pkg>": "<spec>" }, "assets"? { ... } }
```

`orion.lock`:

```
{ "packages": { "<name>": { "version", "resolved", "sha256", "source" } } }
```

## Versiones

Una dependencia admite `*` o `latest` (cualquiera), una versión exacta
(`1.2.3`), caret (`^1.2.3`, mismo major), tilde (`~1.2.3`, mismo major.minor) y
`>=`, `>`, `<=`, `<`. Es un subconjunto deliberado de semver.

## Firmas

Los assets nativos pueden llevar una firma RSA PKCS#1 v1.5 sobre su SHA-256.
Sin claves de confianza instaladas se avisa y se continúa: el sha256 ya
garantiza que el binario es el que declara el registro, y la firma solo añade
quién lo declara.
