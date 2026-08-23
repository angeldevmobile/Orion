# Changelog — Orion Language

Los cambios notables del lenguaje, la stdlib y las herramientas. Fechas en
formato AAAA-MM-DD.

## 2026-08-23

### Añadido
- **`browser` entra en el shadow DOM.** Los selectores atraviesan las shadow
  roots abiertas a cualquier profundidad, igual que ya atravesaban los iframes.
  Un componente web guarda su contenido en una shadow root y el
  `querySelector` del documento no entra: el selector correcto "no existe" y no
  hay pista de por qué. Media web moderna es exactamente eso.

  Entra la búsqueda **y el clic**: el hit-test baja por las shadow roots,
  porque `elementFromPoint` devuelve el host y `host.contains(boton)` es false
  —`contains` no cruza la frontera—, así que sin esto todo componente parecería
  tapado por sí mismo y `click` fallaría con un motivo imposible de entender.

  Las roots cerradas (`mode: 'closed'`) no son accesibles ni para el navegador:
  `exists` dice `no`, que es la respuesta honesta. El caso normal no paga el
  recorrido (3 ms en una página de 500 filas), y se apaga con
  `open({ shadow: no })`.

- **`browser.route` — intercepción de peticiones.** `watch`/`capture` miraban
  la red; ahora se puede decidir:

  ```orion
  web.route(p, "*/api/stock*", { mock: { status: 500, json: { "error": "caido" } } })
  web.route(p, "*.png",        { block: yes })
  web.route(p, "*/api/*",      { headers: { Authorization: "Bearer " + token } })
  web.route(p, "*/lento*",     { fail: "timedout" })
  ```

  Con esto se puede probar el camino de error sin tocar el servidor, trabajar
  con el backend a medias, quitarse de encima lo que no se mira, y autenticarse
  donde no hay formulario. `{ times: n }` dispara solo las n primeras veces, que
  es la única forma de comprobar que un reintento reintenta.

  Las reglas se prueban en orden y manda la primera que casa, como en un
  cortafuegos. `unroute` las quita y `routes` dice cuántas veces ha disparado
  cada una. La lista blanca de `open({ allow })` se comprueba **antes**: un
  `mock` no puede reabrir un dominio cerrado a propósito.

- **`browser.emulate` — dispositivo, idioma, zona horaria y ubicación.**
  Presets (`iphone`, `ipad`, `android`, `laptop`, `desktop`) que son un punto de
  partida, no una lista cerrada: cualquier campo se sobrescribe en la misma
  llamada. Sin esto no se pueden automatizar los sitios que sirven otro HTML al
  móvil, ni reproducir un fallo que depende de la zona horaria, ni evitar que el
  `Accept-Language` del contenedor de CI cambie los textos.

  Poner `geo` concede el permiso de geolocalización solo: sin ello la página
  recibe `PERMISSION_DENIED` y la posición emulada no llega a usarse nunca.

- **`browser.cookies` / `set_cookie` / `clear_cookies`**, para cuando
  `save_state`/`load_state` (la sesión entera) es demasiado.

- **`fn main()` se llama sola.** Un programa cuyo código entero vivía dentro de
  `main` terminaba con éxito, sin salida y sin aviso: el peor fallo posible,
  porque no se parece a un fallo. Ahora la llamada se añade si el programa
  define `main` y **no la nombra en ninguna parte** —ni a nivel superior, ni
  desde otra función, ni pasándola como valor—, así que los programas que ya
  escribían `main()` a mano siguen ejecutándose una sola vez.

  Los módulos cargados con `use` no pasan por ahí: su `main` no debe correr al
  importarlos. El REPL tampoco. Un `main` con parámetros obligatorios no se
  puede llamar sin argumentos, y en vez de callarse lo dice.

### Cambiado
- **Los mensajes de error están en inglés**, como el resto del lenguaje. Eran
  ~920 cadenas repartidas por el núcleo (VM, value, named args, pkg, JIT/AOT),
  el módulo `browser` entero —incluido el JavaScript que se inyecta en la
  página— y la librería estándar. La traza de pila (`at f (line 3)`) y el
  prefijo que el renderizador de errores parsea cambiaron con ellas.

  Quedan en español los nombres de los alias obsoletos (`db.insertar`,
  `cache.guardar`), que son nombres y no texto, y el catálogo de documentación
  que alimenta el hover de la extensión.

## 2026-08-20

### Añadido
- **`SPEC.md` — especificación del lenguaje, y es ejecutable**: 11 secciones
  derivadas del compilador (lexer, parser, typechecker, VM), no de la memoria.
  Cubre estructura léxica, comentarios, identificadores, keywords, literales,
  precedencia completa de 14 niveles, el desugar de `|>`, resolución de
  nombres, valores función, y semántica de evaluación.

  Lo que la hace distinta de un documento: `tests/spec_examples.orx` +
  `tests/spec_conformance.rs` ejecutan **45 afirmaciones** con valor exacto, y
  el paso está en CI. Si el compilador cambia, el fallo no dice "algo se rompió",
  dice `power_asocia_derecha: SPEC dice '512', el compilador da '64'`.

  Escribirla desmintió tres cosas que se daban por ciertas: `**` asocia a la
  derecha, `type()` de una función nombrada devuelve `string` (una función
  nombrada **es** el string de su nombre, y `greet == "greet"` es `yes`), y la
  notación exponencial sí existe. También dejó fijado que `null` y `undefined`
  son el mismo valor, que `/` es división real, que el overflow es error, y que
  `for .. in` no itera dicts.

### Añadido
- **`orion check` avisa de los alias españoles obsoletos**, con el nombre inglés
  que los sustituye:

  ```
  !  [deprecated] line 1 — use "formato" uses a deprecated Spanish module name;
                           write use "format" instead — see SPEC.md section 11.
  !  [deprecated] line 6 — db.insertar() is a deprecated Spanish alias of
                           db.insert(). It still works, but it is scheduled for
                           removal — see SPEC.md section 11.
  ```

  Sin esto, la retirada anunciada en SPEC.md §11 era una trampa: el usuario se
  enteraría el día que su programa dejara de compilar. Los avisos van con `kind`
  propio (`deprecation`, no `warning`) y se enseñan **siempre**, con o sin
  `--types`; esconder una deprecación tras un flag es no avisar. `orion run` no
  los muestra, para no dar la lata en cada ejecución.

  La tabla vive en `src/deprecated.rs`, **144 entradas curadas a mano**. No se
  deriva del registro a propósito: el registro marca todos los alias, pero
  `log.warn` comparte brazo con `log.info` sin estar obsoleto, y
  `state.increment` es alias inglés de `state.incr`. Avisar de esos sería
  decirle al usuario que su código está obsoleto cuando no lo está. `df` y
  `embeddings` tampoco están: son abreviaturas inglesas deliberadas.

  `tests/deprecated_sync.rs` comprueba que cada entrada siga existiendo en el
  registro, que su destino inglés exista (un aviso que manda a un nombre
  inexistente es peor que no avisar) y que la tabla esté ordenada, porque la
  búsqueda es por bisección.

### Cambiado
- **Toda la salida del CLI está en inglés**: ayuda, banner, `doctor`, `check`,
  `fmt`, `test`, `watch`, `bench`, `build`, `docs`, `new` y el debugger
  interactivo. Antes el comando que enseñaba las deprecaciones en inglés las
  rodeaba de español (`Verificando:`, `sin errores`), que era la peor mezcla
  posible.

  Fuera de esta tanda a propósito: `builtins.rs` y `builtins_gen.rs`, que no son
  salida del CLI sino **documentación de la stdlib** (352 entradas generadas
  desde los comentarios de contrato de los módulos). Se traducen junto con los
  671 mensajes de `modules/`, que es el mismo eje.

- **Los mensajes de error del núcleo están en inglés**: lexer, parser, codegen,
  typechecker, VM y las cinco etiquetas de `error.rs` (`lexical error`,
  `syntax error`, `compile error`, `type error`, `runtime error`). Es lo primero
  que ve alguien que escribe mal una línea, y hasta ahora le contestaba en
  español aunque el lenguaje se anunciara en inglés.

  ```
  antes:  error léxico   → Comentario inválido '//'. Usa '--' para comentarios
  ahora:  lexical error  → Invalid comment '//'. Use '--' for comments
  ```

  Ojo con dos que no son lo que parecen y se tradujeron a mano: `Módulo por
  cero` y `Módulo solo soporta enteros` hablan del operador `%`, no de un módulo
  de la stdlib. Quedan **671 mensajes en `modules/`** sin traducir, que es la
  siguiente tanda.

### Arreglado
- **Una lambda dentro de una interpolación `${...}` no compilaba**:
  `show "${apply(fn(x) { return x + 1 }, 10)}"` moría con
  `Función '__lambda_2__' no definida`. El compilador junta los cuerpos de
  lambda generados en un vector `extra_fns` que sube hasta quien registra las
  funciones; `compile_sub_expr`, que compila el trozo de dentro de `${}`,
  se creaba uno **local** y lo descartaba al salir. La llamada quedaba emitida
  y su destino no existía nunca.

  El fallo solo aparecía dentro de un string, así que
  `xs.map(fn(x) { return x * 2 })` funcionaba y
  `"${xs.map(fn(x) { return x * 2 })}"` no. Ahora `extra_fns` se enhebra por
  `compile_interpolated` y `compile_sub_expr`. Cubierto por
  `regression::lambda_dentro_de_interpolacion_se_registra`, que verifica también
  dos lambdas en la misma interpolación, una anidada dentro de otra, la forma
  de flecha y un método con lambda inline.


### Cambiado
- **La API pública de Orion es inglesa**: el inglés pasa a ser la forma canónica
  de la stdlib, y los nombres españoles pasan a **alias obsoletos**. En esta
  versión no se ha renombrado ni eliminado nada: `db.insertar`, `cache.guardar`
  o `validate.requerido` siguen funcionando y lo harán durante toda la 0.1.x.
  Pero quedan fuera de la superficie estable y **está previsto retirarlos en una
  versión futura**; en código nuevo va el inglés. Lo que cambia ya es cuál
  documenta el registro, y con él el hover, el autocompletado,
  `orion --builtins-json` y la referencia del sitio.

  Se aplicó reordenando los nombres dentro de cada brazo del `match` (en Rust el
  orden es indiferente, pero `scripts/gen_builtins.js` toma el primero como
  principal) y volteando el comentario de contrato, que es de donde sale la
  firma documentada. Alcance: **115 brazos** en 20 módulos y **101 comentarios**.

- **Cuatro módulos tenían nombre español sin alternativa** y ahora responden
  también en inglés: `task`/`tarea`, `queue`/`cola`, `format`/`formato`,
  `graph`/`grafo`. Requiere mantener sincronizados cuatro sitios: el dispatch y
  `is_known_module()` en `modules/mod.rs`, `canonical_module()` en el
  typechecker y `canonical()` en `tests/builtins_registry_sync.rs`.

- **Coherencia dentro del propio inglés**: `stream.where` (antes `where_`),
  `stream.zip_lists` (antes `zip_`), `frame.where` (nuevo, `where_` no tenía
  alternativa), `proto.encode_base64`/`decode_base64` (antes solo la forma
  abreviada `_b64`) y `matrix.rot_2d` (antes solo `rot2D`, camelCase en una API
  snake_case). Todas las formas anteriores siguen vivas como alias.

  Se dejan a propósito dos cosas que parecen incoherencias y no lo son:
  `excel_f.if_` lleva guion bajo porque `if` es keyword y `excel_f.if(...)` no
  parsearía; y `quantum.gate_H`/`gate_CNOT` van en mayúscula porque es la
  notación de la física, y además son funciones distintas de `h`/`cnot` (unas
  devuelven la matriz de la puerta, las otras la aplican).

### Roto
- **`type(t)` sobre una tarea devuelve `"task"`, antes `"tarea"`**. Es el único
  cambio de esta tanda que no se puede aliasar, porque un string devuelto no
  tiene alias. Era el único tipo de runtime con nombre español entre diez
  ingleses (`int`, `float`, `string`, `bool`, `list`, `dict`, `ptr`, `null`,
  `fn`, `module`). Un programa que compare `type(t) == "tarea"` debe
  actualizarse.

### Tests
- Suite completa en verde tras el cambio: **421 tests, 0 fallos** (unit,
  regression, differential VM-JIT, typecheck, modules_smoke, concurrency,
  packages_resolution, readme_examples_parse y builtins_registry_sync).
- `tests/test_infra.orx` y `tests/test_utilidades.orx` se dejan **en español a
  propósito**: son la cobertura de regresión que demuestra que los alias siguen
  vivos.

## 2026-08-08

### Arreglado
- **`orion build` — las funciones no veían las variables globales (P0)**: el
  compilador nativo daba a cada función únicamente variables locales de
  Cranelift, así que un nombre definido fuera de ella llegaba como `null`. Solo
  afectaba al **ejecutable compilado**; `orion run` nunca estuvo mal, ni
  siquiera con miles de llamadas calientes, porque ahí manda la VM.

  Lo peligroso era la forma del fallo. A veces se caía y a veces no:

  ```orion
  IVA = 0.21
  fn con_iva(base) { return base * (1 + IVA) }
  show con_iva(100)         -- orion run: 121   |   .exe: otro resultado
  ```

  Y como `use "modulo"` define un global, **cualquier llamada a un módulo dentro
  de una función** moría con `[JIT] CallMethod: tipo no soportado (tag=0)`, un
  mensaje que no apuntaba a la causa: el receptor era el `null` del global que
  no se encontró. En la práctica ningún programa real compilaba, porque todos
  envuelven su lógica en funciones.

  Ahora el runtime del JIT tiene una tabla de globales: el nivel superior
  publica al asignar (y al hacer `use`), y una función lee de ahí los nombres
  que no son suyos. La regla de qué es local se conserva igual que en la VM —
  parámetros, lo que la función asigna, y los campos del shape en el cuerpo de
  un `act`—, así que asignar dentro sigue creando una variable propia sin tocar
  el global. La tabla es de proceso, no por hilo, para que una tarea lanzada con
  `spawn` vea lo mismo que el resto.

- **`orion build` — un programa con `fn main()` no compilaba nativo**: el objeto
  generado comparte espacio de nombres con el `main` de C que arranca el
  ejecutable, así que el símbolo se declaraba dos veces con firmas distintas
  (`i64` contra `i32`), la compilación nativa se abortaba y caía al modo
  bytecode embebido. El binario funcionaba, pero **ninguna aplicación real
  llegaba a compilarse nativa**, porque `fn main()` es la forma natural de
  escribirlas. Los símbolos de usuario ahora se prefijan en AOT; el nombre de
  Orion se conserva para el registro en tiempo de ejecución.

- **`orion build` — un diccionario salía con las claves invertidas**: los pares
  de un literal salen de la pila al revés que en el código, y la VM los voltea
  para conservar el orden de escritura. El JIT decía replicar al intérprete y se
  saltaba justo ese paso, así que `{zeta: 1, alfa: 2}` se convertía en
  `{alfa: 2, zeta: 1}` **solo en el ejecutable compilado**. De ese orden dependen
  cosas que se ven: el JSON generado, las columnas de un CSV, lo que imprime un
  `show` — y el esquema de `browser.extract`, que es un literal y hacía salir los
  registros con los campos al revés. Dos tests nuevos en `differential.rs`.

### Tests
- `aot_native.rs`: seis casos nuevos que cubren el hueco por el que se colaron
  los dos defectos anteriores — un global leído dentro de una función (número,
  cadena y namespace de módulo), que una asignación local no pise el global, que
  el valor visto sea el del momento de la llamada, y que `fn main()` compile
  nativo. La batería anterior solo probaba programas autocontenidos
  (aritmética, recursión, shapes, cadenas), y por eso nadie se enteró.


## 2026-07-15

### Añadido
- **Lenguaje — `with` (recursos con ámbito)**: nueva sintaxis
  `with h = modulo.abrir(...) { ... }` que garantiza `modulo.free(h)` al salir
  del bloque, **también si el cuerpo lanza un error** (se libera y el error se
  re-lanza, capturable por un `attempt` exterior). Funciona con cualquier
  módulo que tenga `free` (frame, serie, quantum…) y con handles string o int
  (conoce el módulo estáticamente, no adivina por el handle). Reglas del
  parser: el inicializador debe ser `modulo.fn(...)`, y `return`/`break`/
  `continue` que escaparían del bloque sin liberar se rechazan en compilación
  con un mensaje claro (los loops internos del cuerpo sí pueden usar break).
  Implementado por desugar a `attempt/handle` en codegen — el JIT hereda la
  semántica sin cambios porque compila desde el mismo bytecode. Soporte
  completo en typechecker (sin falsos positivos), `orion fmt` y resaltado de
  la extensión VSCode.
- **frame — gestión del store**: `frame.free(handle)` (libera un frame; las
  transformaciones crean frames nuevos que antes vivían para siempre — la misma
  fuga que ya se arregló en `serie`) y `frame.frames()` (frames vivos en
  memoria). Imprescindibles en procesos largos (`serve`).
- **Tests**: `tests/test_frame.orx` — barrido funcional e2e del motor de datos
  columnar con valores exactos (17 tests / ~85 checks): inferencia de tipos,
  keep/drop/rename, where_ por tipo, head/tail/sort, estadísticas (std
  poblacional, percentiles interpolados), group, add_col, roundtrips CSV y
  .odf, autodetección de formato, from_txt, scan_stats, each_chunk, salidas
  Excel/odf streaming, free/frames. +1 test de regresión en modules_smoke.

### Arreglado
- **break/continue (P0)**: estaban rotos en TODO el lenguaje — codegen emitía
  `Jump(0)` que nunca se parcheaba, así que `break` y `continue` saltaban a la
  instrucción 0 (reinicio del programa o de la función) y el loop se volvía
  infinito. Ningún test los ejercitaba; lo destapó el barrido de `with`. Ahora
  codegen mantiene una pila de contextos de loop y parchea break → fin del
  loop y continue → re-evaluación de condición (while) o paso de incremento
  (for). `break`/`continue` fuera de un loop son error de compilación con
  mensaje claro. +9 tests de regresión y +3 diferenciales VM/JIT.
- **VM — handlers huérfanos**: un `return` dentro de `attempt` se saltaba el
  `EndAttempt` y su handler quedaba vivo en la pila de errores; un error
  posterior en el caller saltaba a una dirección de otra función (el programa
  podía "terminar" en silencio en vez de reportar). Al morir un frame se
  descartan ahora sus handlers pendientes. +2 tests de regresión.
- **frame.each_chunk**: exigía un tercer argumento `fn` que el código nunca
  llamaba (los scripts pasaban un dummy). Firma real ahora:
  `each_chunk(ruta, chunk_size = 10_000)` → lista de handles, un frame por
  bloque; los argumentos extra se ignoran por compatibilidad.

### Validado
- **GC — ciclos huérfanos**: el fix del 2026-07-11 verificado e2e con el
  binario release: 200k y 1M de ciclos de listas (`push(a,a)`), closures
  (env→lista→closure→env) e instancias (`a.next=b, b.next=a`) → RAM pico
  plana (~11 MB, igual que el control sin ciclos; antes del fix 200k ciclos
  fugaban ~79 MB).

## 2026-07-14

### Añadido
- **GUI — animación y dibujo libre**: `gui.tick(ms)` (evento periódico que
  re-ejecuta el script; los clics tienen prioridad y el reloj se apaga si el
  script deja de pedirlo) y `gui.canvas(w, h) … gui.end()` con formas
  genéricas `circle`, `line`, `rect`, `arrow`, `text_at` (colores temables).
- **Demo**: `demo/demo_bloch_anim.orx` — esfera de Bloch animada con física
  real: cada tick rota el estado cuántico 6° y redibuja desde `q.bloch()`.
- **Typeshed automática**: el generador cubre módulos-directorio (`gui`,
  `tui`) → 875 funciones en 58 módulos, y `build.rs` la regenera en cada
  build (imposible que el hover del LSP quede desfasado del código).

### Arreglado
- **CLI (P1)**: `orion run app.orx` no registraba la ruta del script, por lo
  que toda GUI lanzada con `run` quedaba estática (los eventos no
  re-ejecutaban el script). Solo la invocación directa `orion app.orx`
  activaba el modo reactivo.

## 2026-07-13

### Añadido
- **quantum — simulador de circuitos real**: `circuit(n)` hasta 24 qubits con
  puertas por qubit en O(2^n) (nunca se construye la matriz 2^n×2^n), después
  paralelizadas con rayon (GHZ-20: 11 ms). Puertas `h/x/y/z/sgate/tgate`,
  paramétricas `rx/ry/rz/phase`, multi-qubit `cnot/cz/cphase/swap/ccx`, y
  `ugate`/`cugate` para puertas definidas por el usuario (con validación de
  unitariedad). Medición: `probs`, `sample` (regla de Born), `collapse`
  (mide un qubit y colapsa), `state`, `reset`, `free`.
- **Demo**: `demo/demo_grover.orx` — búsqueda de Grover en Orion puro
  (P(101) = 0.9453125, el valor teórico exacto) y
  `demo/demo_quantum_lab.orx` — laboratorio interactivo de 1 qubit.
- **matrix — álgebra lineal numérica**: motor nalgebra a partir de 32×32
  (mul tipo BLAS, LU con pivoteo; 512×512 ≈ 10× más rápido). Funciones
  nuevas: `solve` (sistemas lineales), `eig` (valores propios),
  `svd` ({u, s, vt}), `rank`, `norm`.
- **serie**: `free(handle)` y `count()` — las transformaciones acumulaban
  handles sin forma de liberarlos en procesos largos.

### Arreglado
- **matrix**: `det` pasó de cofactores O(n!) a eliminación gaussiana con
  pivoteo O(n³) (11×11: 7 s → 1.2 ms); llamadas sin argumentos hacían panic
  de la VM entera (ahora error controlado).
- **quantum**: `qubit(a_re, a_im, b_re, b_im)` ignoraba sus argumentos y
  devolvía siempre |0⟩; ahora los honra, normaliza y rechaza el estado nulo.
- **stat**: `correlation` devolvía 0.9999999999999998 en correlación
  perfecta (ruido f64); ahora redondea a 1e-12.
- **csv**: `stats` interpola percentiles linealmente (mediana de [1,2,3,4]
  = 2.5, consistente con `serie`).
- **fs**: `rmdir` es idempotente (no-existe → `no` en vez de error; permisos
  y bloqueos sí se reportan).

### Validado
- Barrido funcional e2e con valores exactos (~240 checks, 0 fallos) de:
  `matrix`, `serie`, `zip`, `json`, `template`, `csv`, `stat`, `vector`,
  `grafo`, `quantum`. Todos hacen trabajo real (serde, minijinja, deflate,
  petgraph con A*, similitud coseno, Pearson/OLS, vector de estados
  cuántico con interferencia de fases genuina).

## 2026-07-12

### Añadido
- **crypto/crypto2**: AES-256-GCM real (antes XOR), derivación de clave con
  Argon2id + salt (formato versionado con compatibilidad hacia atrás), MD5
  real para checksums, comparación en tiempo constante.
- **Extensión VSCode**: hover estilo Pylance para módulos y `use`, typeshed
  completa autogenerada, `show` multi-argumento, formatter + `fmt --check`,
  lint de indentación engañosa.
- **orion watch**: reinicia servidores (`serve`) como proceso hijo estilo
  nodemon; GUI con hot reload in-process.

### Arreglado
- **ai**: `chat_start(system)` descartaba el system prompt; `set_model()` no
  afectaba a `think/learn/sense` (tenían HTTP propio con un modelo retirado);
  `status()` ahora devuelve dict. Defaults de modelo movidos a alias sin
  fecha (`claude-haiku-4-5`).
