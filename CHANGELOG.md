# Changelog — Orion Language

Los cambios notables del lenguaje, la stdlib y las herramientas. Fechas en
formato AAAA-MM-DD.

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
