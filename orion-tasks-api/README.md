# Orion Tasks API

Un backend **REST CRUD** completo escrito en **Orion**, con persistencia en archivo JSON.
Demuestra que Orion sirve para construir aplicaciones de servidor reales.

## Qué hace

Una API de gestión de tareas con todas las operaciones CRUD, validación de entrada,
códigos de estado HTTP correctos y respuestas JSON. El estado persiste en disco
(`tasks_db.json`) — sobrevive entre peticiones y entre reinicios del servidor.

| Método | Ruta          | Descripción                          | Body                       |
|--------|---------------|--------------------------------------|----------------------------|
| GET    | `/`           | Info de la API                       | —                          |
| GET    | `/health`     | Estado del servicio                  | —                          |
| GET    | `/tasks`      | Lista todas las tareas               | —                          |
| POST   | `/tasks`      | Crea una tarea                       | `{"title": "..."}`         |
| GET    | `/tasks/:id`  | Obtiene una tarea                    | —                          |
| PUT    | `/tasks/:id`  | Actualiza una tarea                  | `{"title"?, "done"?}`      |
| DELETE | `/tasks/:id`  | Elimina una tarea                    | —                          |

Códigos de estado: `200` OK · `201` creado · `400` body inválido · `404` no encontrado · `405` método no permitido.

## Cómo ejecutarlo

```bash
# 1. Arrancar el servidor (escucha en http://localhost:8088)
orion orion-tasks-api/main.orx

# 2. En otra terminal, ejecutar el smoke test del CRUD completo
bash orion-tasks-api/test_api.sh
```

## Ejemplos con curl

```bash
# Listar
curl http://localhost:8088/tasks

# Crear
curl -X POST http://localhost:8088/tasks \
     -H "Content-Type: application/json" \
     -d '{"title":"Escribir el README"}'
# → 201  {"done":false,"id":4,"title":"Escribir el README"}

# Marcar como completada
curl -X PUT http://localhost:8088/tasks/2 \
     -H "Content-Type: application/json" \
     -d '{"done":true}'
# → 200  {"done":true,"id":2,"title":"Construir una API REST"}

# Eliminar
curl -X DELETE http://localhost:8088/tasks/1
# → 200  {"deleted":1}

# Errores
curl -X POST http://localhost:8088/tasks -d '{}'      # → 400
curl http://localhost:8088/tasks/999                  # → 404
```

## Cómo está construido

- **`use net`** — servidor HTTP nativo (`serve PORT router`). El router recibe un
  dict `req` con `path`, `method`, `body` y `params` (query string).
- **`use json`** — `json.parse` (string → valor), `json.forge` (valor → JSON con
  claves ordenadas, determinista), `json.absorb`/`json.emit` (leer/escribir archivo).
- **Persistencia**: como el estado en memoria no sobrevive entre peticiones, la
  fuente de verdad es `tasks_db.json`. Cada petición carga el archivo, opera y lo
  guarda. Las peticiones se procesan en serie, así que no hay condiciones de carrera.
- **Manejo de errores**: `attempt { ... } handle err { ... }` envuelve el parseo de
  JSON y la carga del archivo (primer arranque sin DB → siembra datos de ejemplo).

Todo el backend son ~190 líneas de Orion en [`main.orx`](main.orx).
