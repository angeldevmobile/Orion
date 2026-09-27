# Backlog

Cosas encontradas y no arregladas todavía, con el motivo por el que importan.
Lo que se arregla sale de aquí y entra en [`CHANGELOG.md`](CHANGELOG.md).

## `db.transaction` no deja decidir dentro de la transacción

Recibe una **lista fija** de sentencias y las ejecuta todas: no hay forma de
leer un valor a mitad y ramificar en Orion. Un checkout del tipo "mira el
stock, y si no alcanza aborta" no se puede escribir tal cual.

En la demo `comercio` se resolvió apoyándose en un `CHECK (stock >= 0)` que
aborta la sentencia, lo cual además es mejor diseño. Pero no todo caso se deja
expresar en SQL puro: por ejemplo, cobrar a una pasarela externa entre dos
escrituras.

Camino razonable: una transacción interactiva con `db.begin(url)` que devuelva
un handle y permita `query`/`exec`/`commit`/`rollback` sobre esa misma
conexión, dejando la lista de sentencias como atajo para el caso sencillo.

## En macOS el navegador sobrevive a un `kill`

`browser` ata el navegador a la vida del proceso con un *job object* en
Windows y `PR_SET_PDEATHSIG` en Linux. macOS no tiene equivalente directo, así
que ahí un proceso muerto de golpe sigue dejando el navegador vivo, que es
justo lo que esta función existe para evitar.

Camino posible: un hilo que vigile al padre por `kqueue` (`NOTE_EXIT`) y mate
al navegador, o que el proceso hijo compruebe periódicamente si su padre
cambió de PID.

## Ocho `Result` ignorados

Hay ocho avisos `unused_must_use` en `orion-vm/src/modules/browser/cdp.rs`,
`orion-vm/src/modules/insight_mod.rs` y `orion-vm/tests/browser_e2e.rs`.
`cargo` no los muestra; aparecen en el análisis del editor.

Normalmente se tapan con `let _ =`, pero **antes hay que mirar el de `cdp.rs`**:
está en el camino del transporte y puede estar tragándose un error de envío de
verdad. Los de los tests son ruido.

## Tres formas documentadas de ejecutar un archivo

`orion archivo.orx`, `orion run archivo.orx` y `orion --run archivo.orx`
funcionan las tres, y cada documento usa una distinta: el README de
`examples/orion-tasks-api` usa la primera, `tests/browser_e2e.rs` la segunda y
la ayuda del CLI la tercera.

No es un fallo, pero quien llega nuevo no sabe cuál es la buena. Conviene
elegir una como canónica en el README y la ayuda, y dejar las otras como
aliases sin documentar.

## `env` no se puede usar como alias de módulo

`use "env" as env` falla con `Se esperaba un identificador, pero se encontró
Env`, porque `env` es palabra reservada. El nombre natural del alias es
justamente el del módulo, así que se tropieza a la primera.

El arreglo barato es el mensaje: que el error diga que ese nombre está
reservado y sugiera otro, en vez de hablar de identificadores.

## Los ejemplos están en español y el lenguaje en inglés

El README dice que Orion se escribe en inglés y que los nombres en español son
aliases obsoletos, pero `examples/orion-tasks-api` está escrito y comentado en
español. Para un repositorio público conviene decidir una cosa u otra.
