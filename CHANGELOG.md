# Changelog — Orion Language

Los cambios notables del lenguaje, la stdlib y las herramientas. Fechas en
formato AAAA-MM-DD.

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
