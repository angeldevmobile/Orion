//! Registro central de builtins de Orion — la "typeshed" del lenguaje.
//!
//! El compilador es la ÚNICA fuente de verdad para la documentación de keywords,
//! funciones globales, métodos de tipos (list/string/dict) y funciones de módulo.
//! `orion --builtins-json` lo serializa y la extensión de VS Code (LSP) lo lee al
//! arrancar → hover/autocompletado SIEMPRE completos y en sync, sin JSON a mano.
//!
//! Para documentar algo nuevo: agrega una entrada aquí y reconstruye. La extensión
//! lo recoge sola (igual que un docstring de Python lo recogen todas las tools).

use serde::Serialize;

/// Un parámetro de una firma, ya desmenuzado.
///
/// La firma se escribe una sola vez, en el comentario de contrato del módulo, y
/// de ahí sale tanto el texto que se enseña como esto. Tenerlo estructurado es
/// lo que permite que el editor pinte cada parámetro por separado y que el
/// chequeo de tipos pueda mirarlos, en vez de tener una cadena que solo sirve
/// para mostrar.
#[derive(Serialize, Debug, Clone, PartialEq)]
pub struct ParamDoc {
    pub name: String,
    /// `None` cuando el contrato no declara tipo, que es el caso mayoritario
    /// todavía. Sin tipo no se puede comprobar nada, y eso es correcto: es mejor
    /// no saber que inventarse un tipo.
    #[serde(rename = "type")]
    pub ty: Option<String>,
    /// Escrito `nombre?` en el contrato.
    pub optional: bool,
}

/// Desmenuza `mod.fn(a: string, b?: int, c) -> tipo` en sus partes.
///
/// Devuelve `(params, retorno)`. Se separa por las comas de PRIMER nivel: hay
/// firmas con listas y dicts dentro —`gui.fields([["Etiqueta", "valor"], …],
/// opts?)`— y partir por todas las comas las trocearía por la mitad.
pub fn parse_signature(signature: &str) -> (Vec<ParamDoc>, Option<String>) {
    let abre = match signature.find('(') { Some(i) => i, None => return (Vec::new(), None) };
    let cierra = match signature.rfind(')') { Some(i) if i > abre => i, _ => return (Vec::new(), None) };

    let retorno = signature[cierra + 1..]
        .split("->")
        .nth(1)
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty());

    let dentro = &signature[abre + 1..cierra];
    let mut params = Vec::new();
    let mut profundidad = 0i32;
    let mut actual = String::new();

    for c in dentro.chars() {
        match c {
            '(' | '[' | '{' => { profundidad += 1; actual.push(c); }
            ')' | ']' | '}' => { profundidad -= 1; actual.push(c); }
            ',' if profundidad == 0 => {
                if let Some(p) = parse_param(&actual) { params.push(p); }
                actual.clear();
            }
            _ => actual.push(c),
        }
    }
    if let Some(p) = parse_param(&actual) { params.push(p); }

    (params, retorno)
}

/// ¿Lo que hay tras los dos puntos es un tipo, o es prosa?
///
/// No todas las firmas son contratos: algunas usan `clave: valor` para enseñar
/// las opciones de un dict —`excel.long(datos, keep: [campos], var:
/// "nombre_col")`— y tomarlo por un tipo producía cosas como
/// `'"nombre_val") Convierte formato ancho a largo'`. Un tipo de verdad es una
/// palabra, quizá con corchetes (`list[int]`); en cuanto aparecen comillas,
/// espacios o paréntesis, es texto y se descarta. Mejor sin tipo que con uno
/// inventado: un tipo falso ensucia el hover y, ahora que el chequeo los mira,
/// generaría avisos sobre código correcto.
fn es_tipo(t: &str) -> bool {
    !t.is_empty()
        && t.len() <= 24
        && t.chars().all(|c| c.is_alphanumeric() || matches!(c, '_' | '[' | ']' | '.' | '?'))
}

fn parse_param(trozo: &str) -> Option<ParamDoc> {
    let t = trozo.trim();
    // `…` y `...` marcan "y más de lo mismo": no son un parámetro con nombre.
    if t.is_empty() || t == "…" || t == "..." { return None; }

    let (nombre, ty) = match t.split_once(':') {
        Some((n, ty)) => (n.trim(), Some(ty.trim().to_string()).filter(|x| es_tipo(x))),
        None => (t, None),
    };
    let optional = nombre.ends_with('?');
    Some(ParamDoc {
        name: nombre.trim_end_matches('?').trim().to_string(),
        ty,
        optional,
    })
}

#[derive(Serialize)]
pub struct BuiltinDoc {
    /// Nombre simple: "field", "map", "len", "fn".
    pub name: String,
    /// Nombre cualificado para lookup exacto: "gui.field", "list.map", "len".
    pub qualified: String,
    /// Dueño: módulo ("gui","state"…) o tipo ("list","string","dict") o "" (global/keyword).
    pub owner: String,
    /// "keyword" | "builtin" | "method" | "module".
    pub kind: String,
    /// Firma legible: `gui.field(placeholder, estilo?) → null`.
    pub signature: String,
    /// Descripción (una o dos frases).
    pub description: String,
    /// Ejemplo de uso ("" si no aplica).
    pub example: String,
    /// Parámetros ya desmenuzados de `signature`. Se rellenan solos en
    /// `registry()`: nadie los escribe a mano, así que no pueden desincronizarse
    /// del texto que se muestra.
    #[serde(default)]
    pub params: Vec<ParamDoc>,
    /// Tipo de retorno declarado, si el contrato lo dice.
    #[serde(default)]
    pub returns: Option<String>,
}

fn e(name: &str, qualified: &str, owner: &str, kind: &str, signature: &str, description: &str, example: &str) -> BuiltinDoc {
    BuiltinDoc {
        name: name.into(),
        qualified: qualified.into(),
        owner: owner.into(),
        kind: kind.into(),
        signature: signature.into(),
        description: description.into(),
        example: example.into(),
        // Se derivan de la firma en `registry()`, de una vez para todas las
        // entradas vengan de donde vengan (curadas a mano o generadas).
        params: Vec::new(),
        returns: None,
    }
}

/// Atajo para keywords (owner vacío, sin firma de llamada).
fn kw(name: &str, signature: &str, description: &str, example: &str) -> BuiltinDoc {
    e(name, name, "", "keyword", signature, description, example)
}

/// Atajo para funciones globales (len, str, show…).
fn g(name: &str, signature: &str, description: &str, example: &str) -> BuiltinDoc {
    e(name, name, "", "builtin", signature, description, example)
}

/// Atajo para método de tipo (list/string/dict). qualified = "owner.name".
fn m(owner: &str, name: &str, signature: &str, description: &str, example: &str) -> BuiltinDoc {
    e(name, &format!("{owner}.{name}"), owner, "method", signature, description, example)
}

/// Atajo para función de módulo. qualified = "module.name".
fn f(module: &str, name: &str, signature: &str, description: &str, example: &str) -> BuiltinDoc {
    e(name, &format!("{module}.{name}"), module, "module", signature, description, example)
}

pub fn registry() -> Vec<BuiltinDoc> {
    let mut v = Vec::new();
    keywords(&mut v);
    globals(&mut v);
    list_methods(&mut v);
    string_methods(&mut v);
    dict_methods(&mut v);
    modules(&mut v);
    math_module(&mut v);

    // Resto de la stdlib: entradas extraídas de los match-arms de src/modules/*.rs
    // (node scripts/gen_builtins.js). Lo curado a mano arriba tiene prioridad:
    // se descarta todo qualified ya presente.
    let seen: std::collections::HashSet<String> = v.iter().map(|e| e.qualified.clone()).collect();
    let mut generated = Vec::new();
    super::builtins_gen::generated_modules(&mut generated);
    v.extend(generated.into_iter().filter(|e| !seen.contains(&e.qualified)));

    // Un solo sitio donde se desmenuza la firma, después de juntarlo todo. Así
    // da igual si la entrada se escribió a mano arriba o la sacó el generador de
    // los comentarios: las dos acaban con los mismos datos estructurados, y no
    // hay forma de que el texto y las partes digan cosas distintas.
    for doc in v.iter_mut() {
        let (params, returns) = parse_signature(&doc.signature);
        doc.params = params;
        doc.returns = returns;
    }
    v
}

/// Módulo `math`: vive en la VM (builtin_math_module en vm.rs), no en
/// src/modules/, así que el generador no lo ve — se documenta aquí.
fn math_module(v: &mut Vec<BuiltinDoc>) {
    for (name, sig, desc) in [
        ("PI",  "math.PI → float",  "Constante π (3.14159…)."),
        ("E",   "math.E → float",   "Constante e (2.71828…)."),
        ("TAU", "math.TAU → float", "Constante τ = 2π."),
        ("PHI", "math.PHI → float", "Razón áurea (1.61803…)."),
        ("INF", "math.INF → float", "Infinito positivo."),
        ("sqrt",      "math.sqrt(x) → float",        "Raíz cuadrada."),
        ("abs",       "math.abs(x) → number",        "Valor absoluto."),
        ("floor",     "math.floor(x) → int",         "Redondea hacia abajo."),
        ("ceil",      "math.ceil(x) → int",          "Redondea hacia arriba."),
        ("round",     "math.round(x) → int",         "Redondea al entero más cercano."),
        ("sin",       "math.sin(x) → float",         "Seno (radianes)."),
        ("cos",       "math.cos(x) → float",         "Coseno (radianes)."),
        ("tan",       "math.tan(x) → float",         "Tangente (radianes)."),
        ("log",       "math.log(x) → float",         "Logaritmo natural."),
        ("log10",     "math.log10(x) → float",       "Logaritmo base 10."),
        ("log2",      "math.log2(x) → float",        "Logaritmo base 2."),
        ("exp",       "math.exp(x) → float",         "e elevado a x."),
        ("pow",       "math.pow(a, b) → float",      "Potencia a^b."),
        ("max",       "math.max(a, b) → number",     "Máximo de dos valores."),
        ("min",       "math.min(a, b) → number",     "Mínimo de dos valores."),
        ("clamp",     "math.clamp(x, lo, hi) → number", "Acota x al rango [lo, hi]."),
        ("factorial", "math.factorial(n) → int",     "Factorial de n."),
        ("sign",      "math.sign(x) → int",          "Signo: -1, 0 o 1."),
        ("degrees",   "math.degrees(r) → float",     "Radianes → grados."),
        ("radians",   "math.radians(d) → float",     "Grados → radianes."),
        ("hypot",     "math.hypot(a, b) → float",    "Hipotenusa √(a²+b²)."),
        ("rand",      "math.rand() → float",         "Float aleatorio en [0, 1)."),
        ("randint",   "math.randint(a, b) → int",    "Entero aleatorio en [a, b]."),
    ] {
        v.push(f("math", name, sig, desc, ""));
    }
}

fn keywords(v: &mut Vec<BuiltinDoc>) {
    v.push(kw("fn", "fn nombre(param: tipo) -> tipo { ... }", "Define una función. Los tipos son opcionales.", "fn suma(a: int, b: int) -> int {\n  return a + b\n}"));
    v.push(kw("async", "async fn nombre(...) { ... }", "Define una función asíncrona (se invoca con `await`).", "async fn bajar(url) {\n  return await net.reach(url)\n}"));
    v.push(kw("await", "await expr", "Espera el resultado de una operación asíncrona.", "datos = await bajar(\"https://...\")"));
    v.push(kw("if", "if cond { ... } else { ... }", "Ejecuta un bloque si la condición es verdadera.", "if x > 0 { show(\"positivo\") }"));
    v.push(kw("else", "else { ... }", "Bloque alternativo de un `if`.", "if ok { a() } else { b() }"));
    // La rama intermedia es `else if`, dos tokens: el parser acepta Else
    // seguido de If. No existe una keyword `elsif`; ofrecerla en el
    // autocompletado hacía que quien la aceptara escribiera código que el
    // lexer rechaza, y con un error desorientador ("se esperaba clave de
    // diccionario", porque el bloque se lee como un dict).
    v.push(kw("else if", "else if cond { ... }", "Rama intermedia entre `if` y `else`.", "if x > 0 { } else if x < 0 { } else { }"));
    v.push(kw("for", "for item in coleccion { ... }", "Itera sobre una lista, string o rango.", "for t in tareas { show(t) }"));
    v.push(kw("while", "while cond { ... }", "Repite mientras la condición sea verdadera.", "while n > 0 { n = n - 1 }"));
    v.push(kw("in", "for x in xs", "Operador de pertenencia/iteración usado con `for`.", "for c in \"hola\" { show(c) }"));
    v.push(kw("return", "return expr", "Devuelve un valor desde una función.", "return a + b"));
    v.push(kw("break", "break", "Sale del bucle actual.", "for x in xs { if x==0 { break } }"));
    v.push(kw("continue", "continue", "Salta a la siguiente iteración del bucle.", "for x in xs { if x<0 { continue } }"));
    v.push(kw("const", "const NOMBRE = valor", "Declara un valor inmutable.", "const PI = 3.1416"));
    v.push(kw("shape", "shape Nombre { campo: tipo ... }", "Define un tipo con campos y acciones (OOP de Orion).", "shape Punto {\n  x: int\n  y: int\n}"));
    v.push(kw("act", "act nombre(self, ...) { ... }", "Método (acción) dentro de un `shape`.", "act distancia(self) {\n  return sqrt(self.x*self.x + self.y*self.y)\n}"));
    v.push(kw("on_create", "on_create(self, ...) { ... }", "Constructor de un `shape`, se llama al instanciar.", "on_create(self, x, y) {\n  self.x = x\n  self.y = y\n}"));
    v.push(kw("use", "use \"modulo\" as alias", "Importa un módulo de la stdlib o un archivo .orx.", "use \"gui\" as gui"));
    v.push(kw("attempt", "attempt { ... } handle e { ... }", "Bloque protegido: captura errores en `handle`.", "attempt {\n  riesgoso()\n} handle err {\n  show(err)\n}"));
    v.push(kw("handle", "handle err { ... }", "Captura el error de un bloque `attempt`.", "attempt { x() } handle e { show(e) }"));
    v.push(kw("serve", "serve(puerto, handler)", "Levanta un servidor HTTP concurrente (pool de hilos).", "serve(8080, router)"));
    v.push(kw("think", "think { ... }", "Bloque de razonamiento/IA (think/sense).", "think { \"resume esto\" }"));
    v.push(kw("match", "match valor { patron -> ... }", "Coincidencia de patrones.", "match x {\n  0 -> \"cero\"\n  _ -> \"otro\"\n}"));
    v.push(kw("yes", "yes", "Literal booleano verdadero (equivale a `true`).", "activo = yes"));
    v.push(kw("no", "no", "Literal booleano falso (equivale a `false`).", "activo = no"));
    v.push(kw("null", "null", "Ausencia de valor.", "x = null"));
    v.push(kw("and", "a and b", "Y lógico (también `&&`).", "if a and b { ... }"));
    v.push(kw("or", "a or b", "O lógico (también `||`).", "if a or b { ... }"));
    v.push(kw("not", "not a", "Negación lógica (también `!`).", "if not listo { ... }"));
    v.push(kw("self", "self.campo", "Referencia a la instancia actual dentro de un `act`.", "self.total = 0"));
}

fn globals(v: &mut Vec<BuiltinDoc>) {
    v.push(g("show", "show(valor, ...) → null", "Imprime en stdout (el `print` de Orion).", "show(\"hola\", 42)"));
    v.push(g("len", "len(x) → int", "Longitud de una lista, string o dict.", "len([1,2,3])  -- 3"));
    v.push(g("str", "str(val) → string", "Convierte un valor a string.", "str(42)  -- \"42\""));
    v.push(g("int", "int(val) → int", "Convierte a entero.", "int(\"42\")  -- 42"));
    v.push(g("float", "float(val) → float", "Convierte a flotante.", "float(\"3.14\")"));
    v.push(g("bool", "bool(val) → bool", "Convierte a booleano.", "bool(0)  -- no"));
    v.push(g("type", "type(val) → string", "Nombre del tipo de un valor.", "type([1,2])  -- \"list\""));
    v.push(g("range", "range(n) | range(ini, fin, paso?) → list", "Genera una lista de enteros.", "range(3)  -- [0,1,2]"));
    v.push(g("list", "list(x) → list", "Convierte un iterable a lista.", "list(\"ab\")  -- [\"a\",\"b\"]"));
    v.push(g("keys", "keys(dict) → list", "Claves de un diccionario.", "keys({\"a\":1})  -- [\"a\"]"));
    v.push(g("values", "values(dict) → list", "Valores de un diccionario.", "values({\"a\":1})  -- [1]"));
    v.push(g("abs", "abs(x) → number", "Valor absoluto.", "abs(-5)  -- 5"));
    v.push(g("max", "max(...args) → number", "Máximo de los argumentos.", "max(3, 9, 1)  -- 9"));
    v.push(g("min", "min(...args) → number", "Mínimo de los argumentos.", "min(3, 9, 1)  -- 1"));
    v.push(g("sum", "sum(lista) → number", "Suma de los elementos.", "sum([1,2,3])  -- 6"));
    v.push(g("round", "round(f, decimales?) → number", "Redondea a N decimales.", "round(3.14159, 2)  -- 3.14"));
    v.push(g("floor", "floor(x) → int", "Redondea hacia abajo.", "floor(3.9)  -- 3"));
    v.push(g("ceil", "ceil(x) → int", "Redondea hacia arriba.", "ceil(3.1)  -- 4"));
    v.push(g("sqrt", "sqrt(x) → float", "Raíz cuadrada.", "sqrt(9)  -- 3.0"));
    v.push(g("pow", "pow(base, exp) → float", "Potencia.", "pow(2, 10)  -- 1024"));
    v.push(g("upper", "upper(s) → string", "Mayúsculas.", "upper(\"hola\")  -- \"HOLA\""));
    v.push(g("lower", "lower(s) → string", "Minúsculas.", "lower(\"HOLA\")  -- \"hola\""));
    v.push(g("strip", "strip(s) → string", "Quita espacios al inicio y fin.", "strip(\"  hi  \")  -- \"hi\""));
    v.push(g("reverse", "reverse(x) → any", "Invierte un string o lista.", "reverse([1,2,3])  -- [3,2,1]"));
    v.push(g("input", "input(prompt?) → string", "Lee una línea de stdin.", "nombre = input(\"Nombre: \")"));
    v.push(g("assert", "assert(cond, msg?) → null", "Falla con error si la condición es falsa.", "assert(x > 0, \"x debe ser positivo\")"));
}

fn list_methods(v: &mut Vec<BuiltinDoc>) {
    v.push(m("list", "push", "lista.push(item) → list", "Agrega al final (MUTA la lista in-place).", "xs.push(9)"));
    v.push(m("list", "append", "lista.append(item) → list", "Alias de push: agrega al final (muta).", "xs.append(9)"));
    v.push(m("list", "len", "lista.len() → int", "Cantidad de elementos.", "xs.len()"));
    v.push(m("list", "is_empty", "lista.is_empty() → bool", "¿La lista está vacía?", "if xs.is_empty() { ... }"));
    v.push(m("list", "first", "lista.first() → any", "Primer elemento (o null).", "xs.first()"));
    v.push(m("list", "last", "lista.last() → any", "Último elemento (o null).", "xs.last()"));
    v.push(m("list", "reverse", "lista.reverse() → list", "Invierte in-place.", "xs.reverse()"));
    v.push(m("list", "contains", "lista.contains(item) → bool", "¿Contiene el elemento?", "xs.contains(3)"));
    v.push(m("list", "join", "lista.join(sep?) → string", "Une los elementos en un string.", "[\"a\",\"b\"].join(\"-\")  -- \"a-b\""));
    v.push(m("list", "map", "lista.map(fn) → list", "Nueva lista aplicando `fn` a cada elemento.", "[1,2,3].map(fn(x){ return x*2 })"));
    v.push(m("list", "filter", "lista.filter(fn) → list", "Nueva lista con los que cumplen `fn`.", "xs.filter(fn(x){ return x>0 })"));
    v.push(m("list", "reduce", "lista.reduce(fn, inicial) → any", "Acumula los elementos en un valor.", "xs.reduce(fn(a,b){ return a+b }, 0)"));
    v.push(m("list", "sort", "lista.sort() → list", "Ordena in-place (números o strings).", "xs.sort()"));
    v.push(m("list", "sum", "lista.sum() → number", "Suma de los elementos numéricos.", "xs.sum()"));
    v.push(m("list", "min", "lista.min() → any", "Mínimo.", "xs.min()"));
    v.push(m("list", "max", "lista.max() → any", "Máximo.", "xs.max()"));
}

fn string_methods(v: &mut Vec<BuiltinDoc>) {
    v.push(m("string", "split", "texto.split(sep) → list", "Divide el string por el separador.", "\"a:b:c\".split(\":\")  -- [\"a\",\"b\",\"c\"]"));
    v.push(m("string", "slice", "texto.slice(ini, fin?) → string", "Subcadena por índices (en caracteres).", "\"hola\".slice(0, 2)  -- \"ho\""));
    v.push(m("string", "contains", "texto.contains(sub) → bool", "¿Contiene la subcadena?", "\"hola\".contains(\"ol\")"));
    v.push(m("string", "repeat", "texto.repeat(n) → string", "Repite el string n veces.", "\"ab\".repeat(3)  -- \"ababab\""));
    v.push(m("string", "to_int", "texto.to_int() → int", "Convierte a entero (error si no es válido).", "\"42\".to_int()  -- 42"));
    v.push(m("string", "to_float", "texto.to_float() → float", "Convierte a flotante.", "\"3.14\".to_float()"));
    v.push(m("string", "upper", "texto.upper() → string", "Mayúsculas.", "\"hola\".upper()"));
    v.push(m("string", "lower", "texto.lower() → string", "Minúsculas.", "\"HOLA\".lower()"));
    v.push(m("string", "len", "texto.len() → int", "Cantidad de caracteres.", "\"hola\".len()  -- 4"));
}

fn dict_methods(v: &mut Vec<BuiltinDoc>) {
    v.push(m("dict", "keys", "dict.keys() → list", "Lista de claves.", "d.keys()"));
    v.push(m("dict", "values", "dict.values() → list", "Lista de valores.", "d.values()"));
    v.push(m("dict", "get", "dict.get(clave) → any", "Valor de la clave (o null).", "d.get(\"nombre\")"));
    v.push(m("dict", "has_key", "dict.has_key(clave) → bool", "¿Existe la clave? (alias: contains).", "d.has_key(\"id\")"));
    v.push(m("dict", "len", "dict.len() → int", "Cantidad de pares.", "d.len()"));
    v.push(m("dict", "is_empty", "dict.is_empty() → bool", "¿El dict está vacío?", "d.is_empty()"));
}

fn modules(v: &mut Vec<BuiltinDoc>) {
    mod_gui(v);
    mod_state(v);
    mod_json(v);
    mod_fs(v);
    mod_net(v);
    mod_datetime(v);
}

fn mod_gui(v: &mut Vec<BuiltinDoc>) {
    v.push(f("gui", "panel", "gui.panel(titulo, ancho, alto)", "Configura la ventana de la app GUI.", "gui.panel(\"Mi App\", 720, 560)"));
    v.push(f("gui", "theme", "gui.theme({accent, bg, surface, text, rounding, heading...})", "Define el tema: colores, bordes, fuentes. Lo que no fijes cae al default.", "gui.theme({ \"accent\": \"#6c63ff\", \"rounding\": 14 })"));
    v.push(f("gui", "run", "gui.run()", "Abre la ventana (bloqueante). El script se re-ejecuta en cada evento.", "gui.run()"));
    v.push(f("gui", "heading", "gui.heading(texto, estilo?)", "Título grande.", "gui.heading(\"Hola\", \"accent\")"));
    v.push(f("gui", "text", "gui.text(texto, estilo?)", "Texto de cuerpo.", "gui.text(\"contenido\")"));
    v.push(f("gui", "caption", "gui.caption(texto, estilo?)", "Texto pequeño/secundario.", "gui.caption(\"detalle\")"));
    v.push(f("gui", "field", "gui.field(placeholder, {id?, ...estilo})", "Campo de texto. Dale un id estable para leerlo con gui.value.", "gui.field(\"¿Qué hacer?\", { \"id\": \"nueva\" })"));
    v.push(f("gui", "value", "gui.value(id) → string", "Lee el valor actual de un field/toggle/pick/slide.", "txt = gui.value(\"nueva\")"));
    v.push(f("gui", "setval", "gui.setval(id, texto)", "Fija/limpia el valor de un field (p.ej. tras agregar).", "gui.setval(\"nueva\", \"\")"));
    v.push(f("gui", "press", "gui.press(label, {bg?, fg?, event?})", "Botón. Con `event` dispara algo distinto del texto visible.", "gui.press(\"✓\", { \"event\": \"toggle:\" + str(i) })"));
    v.push(f("gui", "ghost", "gui.ghost(label, {color?, event?})", "Botón de contorno (secundario).", "gui.ghost(\"Cancelar\")"));
    v.push(f("gui", "pressed", "gui.pressed(nombre) → bool", "¿Fue ese botón el último evento?", "if gui.pressed(\"agregar\") { ... }"));
    v.push(f("gui", "ev", "gui.ev() → string", "Nombre del último evento disparado.", "ev = gui.ev()"));
    v.push(f("gui", "val", "gui.val(clave, default) → any", "Lee estado de UI efímero (vive entre re-ejecuciones).", "n = gui.val(\"contador\", 0)"));
    v.push(f("gui", "set", "gui.set(clave, valor)", "Escribe estado de UI efímero.", "gui.set(\"contador\", n + 1)"));
    v.push(f("gui", "badge", "gui.badge(texto, estilo?)", "Etiqueta de estado.", "gui.badge(\"completado\", \"success\")"));
    v.push(f("gui", "progress", "gui.progress(valor, color?)", "Barra de progreso (0..1 o 0..100).", "gui.progress(50, \"success\")"));
    v.push(f("gui", "tabs", "gui.tabs([labels], activa?)", "Pestañas; al click dispara el label.", "gui.tabs([\"Todas\",\"Activas\"], \"Todas\")"));
    v.push(f("gui", "image", "gui.image(ruta, ancho?, alto?)", "Imagen png/jpg/bmp/gif (mantiene aspecto).", "gui.image(\"logo.png\", 120)"));
    v.push(f("gui", "modal", "gui.modal(titulo) ... gui.end()", "Diálogo centrado.", "gui.modal(\"Confirmar\")\n  gui.text(\"¿Seguro?\")\ngui.end()"));
    v.push(f("gui", "card", "gui.card({width?, fill?}) ... gui.end()", "Tarjeta contenedora.", "gui.card()\n  gui.text(\"x\")\ngui.end()"));
    v.push(f("gui", "row", "gui.row() ... gui.end()", "Fila horizontal.", "gui.row()\n  gui.text(\"a\")\ngui.end()"));
    v.push(f("gui", "col", "gui.col() ... gui.end()", "Columna vertical.", "gui.col() ... gui.end()"));
    v.push(f("gui", "grid", "gui.grid(n) ... gui.end()", "Rejilla de n columnas iguales.", "gui.grid(4) ... gui.end()"));
    v.push(f("gui", "sidebar", "gui.sidebar(ancho?) ... gui.end()", "Barra lateral fija.", "gui.sidebar(220) ... gui.end()"));
    v.push(f("gui", "zone", "gui.zone({estilo}) ... gui.end()", "Contenedor con estilo libre (borde, pad, rounding).", "gui.zone({ \"border\": \"accent\", \"pad\": 22 }) ... gui.end()"));
    v.push(f("gui", "end", "gui.end()", "Cierra el último contenedor abierto.", "gui.end()"));
    v.push(f("gui", "spacer", "gui.spacer(px?)", "Espacio vertical.", "gui.spacer(12)"));
    v.push(f("gui", "divider", "gui.divider()", "Línea separadora.", "gui.divider()"));
    v.push(f("gui", "avatar", "gui.avatar(texto, tam?, estilo?)", "Avatar circular con iniciales.", "gui.avatar(\"AZ\", 44, \"accent\")"));
    v.push(f("gui", "chart", "gui.chart([dicts], tipo, {x, y, color, height})", "Gráfico: bar|line|area|scatter|pie|hist.", "gui.chart(ventas, \"bar\", { \"x\": \"mes\", \"y\": \"total\" })"));
    v.push(f("gui", "table", "gui.table([dicts], {height?, cols?})", "Tabla de datos.", "gui.table(filas, { \"height\": 200 })"));
}

fn mod_state(v: &mut Vec<BuiltinDoc>) {
    v.push(f("state", "persist", "state.persist(ruta) → bool", "Activa respaldo a disco y carga lo existente (sobrevive reinicios).", "state.persist(\"app.db\")"));
    v.push(f("state", "set", "state.set(clave, valor) → valor", "Guarda un valor en el store compartido.", "state.set(\"tareas\", [])"));
    v.push(f("state", "get", "state.get(clave, default?) → any", "Lee un valor (o el default).", "tareas = state.get(\"tareas\", [])"));
    v.push(f("state", "has", "state.has(clave) → bool", "¿Existe la clave?", "if state.has(\"seeded\") { ... }"));
    v.push(f("state", "incr", "state.incr(clave, delta?) → number", "Incremento ATÓMICO (seguro bajo el pool de hilos).", "n = state.incr(\"visitas\")"));
    v.push(f("state", "decr", "state.decr(clave, delta?) → number", "Decremento atómico.", "state.decr(\"stock\")"));
    v.push(f("state", "delete", "state.delete(clave) → bool", "Elimina una clave.", "state.delete(\"tmp\")"));
    v.push(f("state", "keys", "state.keys() → list", "Todas las claves.", "state.keys()"));
    v.push(f("state", "all", "state.all() → dict", "Snapshot completo del estado.", "state.all()"));
    v.push(f("state", "len", "state.len() → int", "Cantidad de claves.", "state.len()"));
    v.push(f("state", "clear", "state.clear() → bool", "Borra todo el estado.", "state.clear()"));
}

fn mod_json(v: &mut Vec<BuiltinDoc>) {
    v.push(f("json", "parse", "json.parse(texto) → any", "Parsea un string JSON a valor de Orion.", "d = json.parse(\"{\\\"a\\\":1}\")"));
    v.push(f("json", "forge", "json.forge(valor) → string", "Serializa un valor a string JSON (claves ordenadas).", "json.forge({ \"a\": 1 })"));
    v.push(f("json", "validate", "json.validate(obj, schema) → bool", "Valida un valor contra un schema ({campo: tipo}).", "json.validate(usuario, {\"nombre\": \"string\", \"edad\": \"int\"})"));
}

fn mod_fs(v: &mut Vec<BuiltinDoc>) {
    v.push(f("fs", "read", "fs.read(ruta) → string", "Lee un archivo de texto completo.", "contenido = fs.read(\"datos.txt\")"));
    v.push(f("fs", "write", "fs.write(ruta, texto)", "Escribe (sobrescribe) un archivo.", "fs.write(\"out.txt\", \"hola\")"));
    v.push(f("fs", "append", "fs.append(ruta, texto)", "Agrega al final de un archivo.", "fs.append(\"log.txt\", linea)"));
    v.push(f("fs", "exists", "fs.exists(ruta) → bool", "¿Existe el archivo/carpeta?", "if fs.exists(\"db.json\") { ... }"));
    v.push(f("fs", "ls", "fs.ls(ruta?) → list", "Lista el contenido de una carpeta.", "fs.ls(\".\")"));
    v.push(f("fs", "mkdir", "fs.mkdir(ruta)", "Crea una carpeta (y sus padres).", "fs.mkdir(\"salida/reportes\")"));
    v.push(f("fs", "delete", "fs.delete(ruta)", "Elimina un archivo o carpeta.", "fs.delete(\"tmp.txt\")"));
    v.push(f("fs", "is_file", "fs.is_file(ruta) → bool", "¿Es un archivo?", "fs.is_file(\"x\")"));
    v.push(f("fs", "is_dir", "fs.is_dir(ruta) → bool", "¿Es una carpeta?", "fs.is_dir(\"src\")"));
}

fn mod_net(v: &mut Vec<BuiltinDoc>) {
    v.push(f("net", "reach", "net.reach(url) → string", "GET HTTP: descarga el cuerpo de una URL.", "html = net.reach(\"https://ejemplo.com\")"));
    v.push(f("net", "transmit", "net.transmit(url, datos) → string", "POST HTTP con cuerpo.", "net.transmit(url, json.forge(payload))"));
    v.push(f("net", "download", "net.download(url, ruta)", "Descarga una URL a un archivo.", "net.download(url, \"img.png\")"));
    v.push(f("net", "status", "net.status(url) → int", "Código de estado HTTP de una URL.", "net.status(\"https://...\")"));
}

fn mod_datetime(v: &mut Vec<BuiltinDoc>) {
    v.push(f("datetime", "now", "datetime.now() → string", "Fecha y hora actuales.", "datetime.now()"));
    v.push(f("datetime", "today", "datetime.today() → string", "Fecha de hoy.", "datetime.today()"));
    v.push(f("datetime", "timestamp", "datetime.timestamp() → int", "Segundos desde la época.", "datetime.timestamp()"));
}

/// Serializa el registro a JSON y lo imprime por stdout (para el LSP).
pub fn run_builtins_json() {
    let reg = registry();
    match serde_json::to_string(&reg) {
        Ok(s) => println!("{s}"),
        Err(e) => eprintln!("error serializando builtins: {e}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    #[test]
    fn registry_invariants() {
        let reg = registry();
        assert!(reg.len() >= 100, "el registro debe cubrir el grueso de la stdlib");

        // Nombres cualificados únicos (sin duplicados que pisen el lookup del LSP).
        let mut seen = HashSet::new();
        for e in &reg {
            assert!(seen.insert(e.qualified.clone()), "qualified duplicado: {}", e.qualified);
            // Toda entrada tiene firma y descripción.
            assert!(!e.signature.is_empty(), "sin firma: {}", e.qualified);
            assert!(!e.description.is_empty(), "sin descripción: {}", e.qualified);
            // kind válido.
            assert!(
                matches!(e.kind.as_str(), "keyword" | "builtin" | "method" | "module"),
                "kind inválido en {}: {}", e.qualified, e.kind
            );
        }

        // El JSON serializa sin error (lo que consume el LSP).
        assert!(serde_json::to_string(&reg).is_ok());
    }

    #[test]
    fn cubre_la_vitrina_gui_state() {
        let reg = registry();
        let has = |q: &str| reg.iter().any(|e| e.qualified == q);
        // Las funciones usadas en demo_tasks.orx deben estar documentadas.
        for q in ["gui.field", "gui.setval", "gui.press", "gui.progress", "gui.tabs",
                  "state.persist", "state.set", "state.get", "list.map", "string.split"] {
            assert!(has(q), "falta doc de builtin clave: {q}");
        }
    }
}
