#!/usr/bin/env bash
# Smoke test del CRUD de Orion Tasks API.
# Uso:  arranca el server (orion orion-tasks-api/main.orx) y luego:  bash orion-tasks-api/test_api.sh
set -u
BASE="http://localhost:8088"

hr() { echo "──────────────────────────────────────────────"; }
req() { # método ruta [body]
  echo "→ $1 $2 ${3:+(body: $3)}"
  if [ -n "${3:-}" ]; then
    curl -s -m 5 -X "$1" "$BASE$2" -H "Content-Type: application/json" -d "$3"
  else
    curl -s -m 5 -X "$1" "$BASE$2"
  fi
  echo
}

hr; echo "1) Info y health"
req GET /
req GET /health

hr; echo "2) Listar tareas (semilla)"
req GET /tasks

hr; echo "3) Crear una tarea"
req POST /tasks '{"title":"Escribir el README"}'

hr; echo "4) Crear sin title → 400"
req POST /tasks '{}'

hr; echo "5) Obtener tarea 2"
req GET /tasks/2

hr; echo "6) Obtener tarea inexistente → 404"
req GET /tasks/999

hr; echo "7) Marcar tarea 2 como done (PUT)"
req PUT /tasks/2 '{"done":true}'

hr; echo "8) Verificar el cambio persistió"
req GET /tasks/2

hr; echo "9) Eliminar tarea 1 (DELETE)"
req DELETE /tasks/1

hr; echo "10) Lista final"
req GET /tasks
hr
