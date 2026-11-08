//! Fuzzer diferencial VM ↔ JIT (Sprint core-hardening).
//!
//! Genera programas Orion ALEATORIOS pero VÁLIDOS —en el subconjunto que el JIT
//! compila a código nativo: numérico (int/float), strings (concat + igualdad) y
//! booleanos; también decimales, operadores de bits, listas, dicts e
//! instancias, y errores dentro de `attempt`— y verifica que el intérprete
//! (`orion archivo`) y el JIT Cranelift (`orion --jit archivo`) produzcan
//! EXACTAMENTE el mismo stdout y el mismo estado de éxito/fallo. Cualquier
//! divergencia es un bug en uno de los dos backends.
//!
//! A diferencia de `differential.rs` (casos escritos a mano), este caza
//! heisenbugs combinatorios que nadie pensaría en escribir: anidamientos de
//! if/while, mezclas de aritmética con overflow, llamadas a funciones, etc.
//!
//! DETERMINISTA: PRNG con semilla fija → un fallo se reproduce exactamente.
//! Al divergir, imprime la semilla y el programa completo para depurar.
//! Sube el volumen con `ORION_FUZZ_N=5000 cargo test --test differential_fuzz`.
//!
//! Diseño que evita FALSOS positivos:
//!   - Enteros, flotantes, strings y booleanos. Los floats usan el formateo `{}`
//!     de Rust —idéntico en VM y JIT— y se acotan a un rango finito (sin
//!     `inf`/`NaN`): ver `float_literal`. La aritmética/comparación mixta
//!     int↔float concuerda.
//!   - Tipos rastreados por variable (`Ty`): los strings solo aparecen donde
//!     VM≡JIT —concatenación `+`, `==`/`!=` y `show`—, nunca en orden (`<`) ni
//!     aritmética (que AMBOS backends rechazan). Nunca bool en aritmética.
//!   - CLOSURES quedan FUERA: el JIT no hace llamadas indirectas (`c()` con `c`
//!     variable) → siempre cae al intérprete, así que el diferencial sería VM vs
//!     VM (trivial). Su paridad se cubre con tests de salida esperada aparte.
//!   - Bucles `while` con contador acotado → terminación garantizada (sin hangs).
//!   - El overflow y la división/módulo por cero SON válidos: ambos backends
//!     deben coincidir en errar (eso también se verifica).

use std::fs;
use std::process::Command;
use std::sync::atomic::{AtomicU32, Ordering};

static COUNTER: AtomicU32 = AtomicU32::new(0);

// ─────────────────────────────────────────────────────────────────────────────
// PRNG xorshift64* — determinista, sin dependencias.
// ─────────────────────────────────────────────────────────────────────────────
struct Rng(u64);

impl Rng {
    fn new(seed: u64) -> Self {
        Rng(seed ^ 0x9E3779B97F4A7C15 | 1) // nunca 0
    }
    fn next_u64(&mut self) -> u64 {
        let mut x = self.0;
        x ^= x >> 12;
        x ^= x << 25;
        x ^= x >> 27;
        self.0 = x;
        x.wrapping_mul(0x2545F4914F6CDD1D)
    }
    /// Entero en [0, n).
    fn below(&mut self, n: u32) -> u32 {
        (self.next_u64() % n as u64) as u32
    }
    /// true con probabilidad num/den.
    fn chance(&mut self, num: u32, den: u32) -> bool {
        self.below(den) < num
    }
    /// Entero pequeño con sesgo hacia valores chicos, ocasionalmente extremos.
    /// Evita `i64::MIN` como literal: el lexer parsea primero el valor absoluto
    /// `9223372036854775808`, que no cabe en i64 (eso es un bug aparte del lexer,
    /// no del fuzzer diferencial).
    fn int_literal(&mut self) -> i64 {
        // 85% valores chicos → la mayoría de los programas COMPLETAN (ejecución
        // combinatoria rica). El 15% extremo ejercita el overflow, que ambos
        // backends deben rechazar por igual.
        // 5% extremes: at 15% almost every program died of overflow early.
        let v = match self.below(60) {
            0 => i64::MAX,
            1 => i64::MIN + 1,
            2 => self.next_u64() as i64,         // entero arbitrario
            _ => (self.below(40) as i64) - 20,   // chico: -20..19
        };
        if v == i64::MIN { i64::MIN + 1 } else { v }
    }

    /// Literal flotante MODESTO como texto fuente Orion. Rango acotado
    /// (|x| < ~50, exponentes de `**` chicos) → todas las operaciones quedan
    /// FINITAS: nunca aparecen `inf`/`NaN`. Eso evita el único rincón donde VM y
    /// JIT difieren en floats: la comparación con `NaN` (`<=`/`>=` dan `false`
    /// vía `< || ==` en la VM pero `true` vía `partial_cmp→Equal` en el JIT).
    /// Es una frontera conocida, no un falso positivo; mantenerla fuera del
    /// rango deja el resto del dominio float verificable de forma confiable.
    ///
    /// El formateo `{}` de Rust para f64 es BIT A BIT idéntico en ambos backends
    /// (VM: `write!(f,"{}",n)`; JIT `runtime::val_to_display`), así que
    /// `show <float>` jamás diverge por formato. Garantiza punto decimal en el
    /// literal —`format!("{}",0.0)` da "0", que el lexer tomaría como int—.
    fn float_literal(&mut self) -> String {
        let f: f64 = match self.below(8) {
            0 => 0.0,
            1 => 0.5,
            2 => 1.5,
            3 => -0.5,
            4 => 2.25,
            _ => {
                let whole = (self.below(100) as f64) - 50.0;     // -50..49
                let frac = (self.below(10) as f64) / 10.0;       // .0..=.9
                whole + frac
            }
        };
        let s = format!("{}", f);
        if s.contains('.') || s.contains('e') { s } else { format!("{}.0", s) }
    }

    /// Literal de string como texto fuente Orion (entre comillas dobles). Pool de
    /// palabras SIN caracteres que requieran escape (ni comillas, ni `\`, ni
    /// saltos) → el lexer las toma tal cual. Incluye la cadena vacía `""` y `"42"`
    /// (concatenar/mostrar dígitos como texto también debe concordar VM↔JIT).
    fn str_literal(&mut self) -> String {
        const WORDS: &[&str] = &["hola", "orion", "abc", "x", "", "42", "foo", "bar"];
        format!("\"{}\"", WORDS[self.below(WORDS.len() as u32) as usize])
    }

    /// Decimal literal: money-like values, zero (division by zero) and negatives.
    fn dec_literal(&mut self) -> String {
        const FIXED: &[&str] = &["0.1d", "0.2d", "19.99d", "2.50d", "1d", "0d", "-3.10d", "100.005d", "7.5d"];
        if self.chance(1, 2) {
            FIXED[self.below(FIXED.len() as u32) as usize].to_string()
        } else {
            format!("{}.{:02}d", self.below(200) as i64 - 100, self.below(100))
        }
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Generador de programas. Cada variable tiene un TIPO rastreado (`Ty`): numérico
// (int/float, intercambiables) o string. El rastreo es necesario porque string y
// número solo concuerdan VM↔JIT en `+` (concat), `==`/`!=` y `show`; en `<`,
// aritmética, etc. AMBOS backends erran (no es divergencia, pero mataría el
// programa). Los booleanos solo aparecen en condiciones y en `show <bool>`.
// ─────────────────────────────────────────────────────────────────────────────
// Dec never mixes with floats (both engines reject it, the type checker too).
// List holds numbers and decimals; Dict always has numeric keys k1 and k2.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Ty { Num, Str, Dec, List, Dict }

struct Gen {
    rng: Rng,
    out: String,
    vars: Vec<(String, Ty)>,    // variables en scope con su tipo
    funcs: Vec<String>,         // funciones num→num definidas
    indent: usize,
    uid: u32,                   // unique suffix for helper variables
}

const VAR_NAMES: &[&str] = &["a", "b", "c", "d", "e", "f", "g", "h"];

impl Gen {
    fn new(seed: u64) -> Self {
        Gen { rng: Rng::new(seed), out: String::new(), vars: Vec::new(), funcs: Vec::new(), indent: 0, uid: 0 }
    }

    fn next_uid(&mut self) -> u32 {
        self.uid += 1;
        self.uid
    }

    /// Elige al azar el nombre de una variable EN SCOPE del tipo `ty`, o `None`
    /// si no hay ninguna. (Clona los candidatos para no chocar con el préstamo
    /// mutable de `self.rng`.)
    fn pick_var(&mut self, ty: Ty) -> Option<String> {
        let cands: Vec<String> = self.vars.iter()
            .filter(|(_, t)| *t == ty).map(|(n, _)| n.clone()).collect();
        if cands.is_empty() { return None; }
        Some(cands[self.rng.below(cands.len() as u32) as usize].clone())
    }

    fn line(&mut self, s: &str) {
        for _ in 0..self.indent { self.out.push_str("    "); }
        self.out.push_str(s);
        self.out.push('\n');
    }

    // ── Expresiones numéricas (int/float) ──────────────────────────────────
    fn num_expr(&mut self, depth: u32) -> String {
        // En el fondo del árbol, solo hojas (literal o variable numérica).
        if depth == 0 || self.rng.chance(1, 3) {
            if self.rng.chance(1, 2) {
                if let Some(v) = self.pick_var(Ty::Num) { return v; }
            }
            return if self.rng.chance(3, 10) {
                // ~30% literal flotante. La aritmética/comparación mixta int↔float
                // concuerda en ambos backends (suma/resta/mul promueven; `<`,`==`
                // numéricos). Excepciones que NO divergen: `%` con operando float
                // (ambos backends erran igual → acuerdo) y `**` cuyo exponente
                // aquí siempre es un int chico (base float → `powi`).
                self.rng.float_literal()
            } else {
                format!("{}", self.rng.int_literal())
            };
        }
        match self.rng.below(9) {
            7 => self.bits_expr(2),
            8 => self.num_from_other(),
            0..=4 => {
                let op = ["+", "-", "*", "%", "**"][self.rng.below(5) as usize];
                let l = self.num_expr(depth - 1);
                if op == "%" {
                    // `%` on a float is an error in both engines and killed most
                    // programs: int dividend, divisor 0 only 5% of the time.
                    let d = if self.rng.chance(1, 20) { 0 } else { 1 + self.rng.below(19) as i64 };
                    let d = if self.rng.chance(1, 3) { -d } else { d };
                    return format!("(int({}) % {})", l, d);
                }
                let r = if op == "**" {
                    // Exponente pequeño NO negativo (negativo es error en ints).
                    format!("{}", self.rng.below(5))
                } else {
                    self.num_expr(depth - 1)
                };
                format!("({} {} {})", l, op, r)
            }
            5 => {
                // Negación unaria. Si el operando ya empieza con '-' (literal
                // negativo), parentizarlo para no emitir `--` adjacente, que el
                // lexer rechaza.
                let e = self.num_expr(depth - 1);
                if e.starts_with('-') {
                    format!("(-({}))", e)
                } else {
                    format!("(-{})", e)
                }
            }
            _ => {
                // Llamada a función (num→num) si hay alguna; si no, una hoja.
                if !self.funcs.is_empty() {
                    let fi = self.rng.below(self.funcs.len() as u32) as usize;
                    let arg = self.num_expr(depth - 1);
                    format!("{}({})", self.funcs[fi], arg)
                } else {
                    format!("{}", self.rng.int_literal())
                }
            }
        }
    }

    /// Int-only expression with bitwise operators (a float operand would make
    /// both engines fail). A negative shift is a shared error.
    fn bits_expr(&mut self, depth: u32) -> String {
        if depth == 0 || self.rng.chance(1, 3) {
            return format!("{}", self.rng.int_literal());
        }
        let op = ["&", "|", "^", "<<", ">>"][self.rng.below(5) as usize];
        let l = self.bits_expr(depth - 1);
        let r = if op == "<<" || op == ">>" {
            if self.rng.chance(1, 10) { "-1".to_string() } else { format!("{}", self.rng.below(70)) }
        } else {
            self.bits_expr(depth - 1)
        };
        format!("({} {} {})", l, op, r)
    }

    /// A number read from a list, dict or decimal in scope.
    fn num_from_other(&mut self) -> String {
        match self.rng.below(4) {
            0 => { if let Some(l) = self.pick_var(Ty::List) { return format!("len({l})"); } }
            1 => { if let Some(d) = self.pick_var(Ty::Dict) { return format!("{d}.k1"); } }
            2 => { let e = self.dec_expr(1); return format!("float({e})"); }
            _ => { let e = self.dec_expr(1); return format!("int({e})"); }
        }
        format!("{}", self.rng.int_literal())
    }

    // ── Decimals, lists and dicts ──────────────────────────────────────────
    fn dec_expr(&mut self, depth: u32) -> String {
        if depth == 0 || self.rng.chance(1, 3) {
            if self.rng.chance(1, 2) {
                if let Some(v) = self.pick_var(Ty::Dec) { return v; }
            }
            return self.rng.dec_literal();
        }
        match self.rng.below(7) {
            0..=2 => {
                let op = ["+", "-", "*", "/", "%"][self.rng.below(5) as usize];
                let l = self.dec_expr(depth - 1);
                let r = self.dec_expr(depth - 1);
                format!("({} {} {})", l, op, r)
            }
            3 => { let l = self.dec_expr(depth - 1); format!("({} * {})", l, self.rng.below(20)) }
            4 => { let l = self.dec_expr(depth - 1); format!("({} ** {})", l, self.rng.below(4)) }
            5 => { let l = self.dec_expr(depth - 1); format!("round({}, {})", l, self.rng.below(3)) }
            _ => { let l = self.dec_expr(depth - 1); format!("abs({})", l) }
        }
    }

    /// 0-4 elements; a third of the lists also mix in decimals.
    fn list_literal(&mut self) -> String {
        let n = self.rng.below(5);
        let decs = self.rng.chance(1, 3);
        let mut items = Vec::new();
        for _ in 0..n {
            let item = if decs && self.rng.chance(2, 3) { self.rng.dec_literal() } else { self.num_expr(1) };
            items.push(item);
        }
        format!("[{}]", items.join(", "))
    }

    fn list_expr(&mut self, depth: u32) -> String {
        if depth == 0 || self.rng.chance(1, 2) {
            if self.rng.chance(1, 2) {
                if let Some(v) = self.pick_var(Ty::List) { return v; }
            }
            return self.list_literal();
        }
        let l = self.list_expr(depth - 1);
        let r = self.list_expr(depth - 1);
        format!("({} + {})", l, r)
    }

    fn dict_literal(&mut self) -> String {
        let a = self.num_expr(1);
        let b = self.num_expr(1);
        format!("{{\"k1\": {}, \"k2\": {}}}", a, b)
    }

    fn expr_of(&mut self, ty: Ty, depth: u32) -> String {
        match ty {
            Ty::Num => self.num_expr(depth),
            Ty::Str => self.str_expr(depth),
            Ty::Dec => self.dec_expr(depth),
            Ty::List => self.list_expr(2),
            Ty::Dict => match self.pick_var(Ty::Dict) {
                Some(d) if self.rng.chance(1, 3) => d,
                _ => self.dict_literal(),
            },
        }
    }

    /// Wraps a risky `show` so its error goes to stdout and gets compared too.
    fn attempt_show(&mut self, e: &str) {
        self.line(&format!("attempt {{ show {e} }} handle err {{ show err }}"));
    }

    /// One operation on a list, dict or instance; a plain show if none is in scope.
    fn collection_stmt(&mut self) {
        match self.rng.below(8) {
            0 => {
                if let Some(l) = self.pick_var(Ty::List) {
                    let x = if self.rng.chance(1, 3) { self.rng.dec_literal() } else { self.num_expr(1) };
                    let call = if self.rng.chance(1, 2) { format!("push({l}, {x})") } else { format!("{l}.push({x})") };
                    self.line(&call);
                    return;
                }
            }
            1 => {
                if let Some(l) = self.pick_var(Ty::List) {
                    let m = ["sum()", "min()", "max()", "first()", "last()", "len()", "is_empty()"][self.rng.below(7) as usize];
                    self.attempt_show(&format!("{l}.{m}"));
                    return;
                }
            }
            2 => {
                if let Some(l) = self.pick_var(Ty::List) {
                    let f = ["sum", "min", "max", "len", "first", "last"][self.rng.below(6) as usize];
                    self.attempt_show(&format!("{f}({l})"));
                    return;
                }
            }
            3 => {
                if let Some(l) = self.pick_var(Ty::List) {
                    self.line(&format!("attempt {{ {l}.sort() }} handle err {{ show err }}"));
                    self.line(&format!("show {l}"));
                    return;
                }
            }
            4 => {
                if let Some(d) = self.pick_var(Ty::Dict) {
                    let key = ["k1", "k2", "k3"][self.rng.below(3) as usize];
                    let e = self.num_expr(2);
                    let s = if self.rng.chance(1, 2) { format!("{d}.{key} = {e}") } else { format!("{d}[\"{key}\"] = {e}") };
                    self.line(&s);
                    return;
                }
            }
            5 => {
                // Dicts are values: the copy must not see the change.
                if let Some(d) = self.pick_var(Ty::Dict) {
                    let n = self.next_uid();
                    let e = self.num_expr(1);
                    self.line(&format!("cp{n} = {d}"));
                    self.line(&format!("{d}.k2 = {e}"));
                    self.line(&format!("show cp{n}"));
                    self.line(&format!("show {d}"));
                    return;
                }
            }
            6 => {
                let lit = self.dict_literal();
                let src = match self.pick_var(Ty::Dict) {
                    Some(d) => format!("[{d}, {lit}]"),
                    None => format!("[{lit}]"),
                };
                self.line(&format!("for v in {src} {{"));
                self.indent += 1;
                self.line("v.k2 = v.k1 * 2");
                self.line("v.k3 = len(v)");
                self.line("show v");
                self.indent -= 1;
                self.line("}");
                return;
            }
            _ => {
                // Instances are shared: q sees the change made through p.
                let n = self.next_uid();
                let a = self.num_expr(2);
                let b = self.num_expr(2);
                self.line(&format!("p{n} = P({a})"));
                self.line(&format!("q{n} = p{n}"));
                self.line(&format!("p{n}.x = {b}"));
                self.line(&format!("show q{n}.x"));
                return;
            }
        }
        let e = self.num_expr(2);
        self.line(&format!("show {e}"));
    }

    // ── Expresiones de string ──────────────────────────────────────────────
    // Producen SIEMPRE un valor string. La única operación es la concatenación
    // `+` con al menos un operando string (str+str, str+num, num+str): las tres
    // concatenan idéntico en VM (`add`) y JIT (`rt_add` cae al brazo de Str).
    fn str_expr(&mut self, depth: u32) -> String {
        if depth == 0 || self.rng.chance(1, 2) {
            if self.rng.chance(1, 2) {
                if let Some(v) = self.pick_var(Ty::Str) { return v; }
            }
            return self.rng.str_literal();
        }
        match self.rng.below(3) {
            0 => { let l = self.str_expr(depth - 1); let r = self.str_expr(depth - 1); format!("({} + {})", l, r) }
            1 => { let l = self.str_expr(depth - 1); let r = self.num_expr(2);          format!("({} + {})", l, r) }
            _ => { let l = self.num_expr(2);          let r = self.str_expr(depth - 1); format!("({} + {})", l, r) }
        }
    }

    // ── Expresiones booleanas (solo para condiciones) ──────────────────────
    fn bool_expr(&mut self, depth: u32) -> String {
        if depth == 0 || self.rng.chance(1, 3) {
            // ~25%: igualdad de strings (solo `==`/`!=`; el ORDEN de strings lo
            // rechazan AMBOS backends, así que no se genera). El resto: comparación
            // numérica completa (incluye orden).
            if self.rng.chance(1, 4) {
                let op = if self.rng.chance(1, 2) { "==" } else { "!=" };
                return format!("({} {} {})", self.str_expr(1), op, self.str_expr(1));
            }
            let op = ["==", "!=", "<", "<=", ">", ">="][self.rng.below(6) as usize];
            let l = self.num_expr(2);
            let r = self.num_expr(2);
            return format!("({} {} {})", l, op, r);
        }
        match self.rng.below(3) {
            0 => {
                let l = self.bool_expr(depth - 1);
                let r = self.bool_expr(depth - 1);
                let op = if self.rng.chance(1, 2) { "and" } else { "or" };
                format!("({} {} {})", l, op, r)
            }
            1 => {
                let e = self.bool_expr(depth - 1);
                format!("(not {})", e)
            }
            _ => {
                let op = ["==", "!=", "<", "<=", ">", ">="][self.rng.below(6) as usize];
                format!("({} {} {})", self.num_expr(2), op, self.num_expr(2))
            }
        }
    }

    /// Elige el nombre de la variable a asignar. Devuelve `(nombre, tipo, es_nueva)`
    /// SIN registrarla todavía: el RHS se genera después con las variables ya en
    /// scope (evita auto-referencia/use-before-def). Si el nombre YA existe,
    /// conserva su tipo (nunca lo cambia → el rastreo de tipos no se desincroniza
    /// del runtime aunque la reasignación ocurra dentro de una rama no tomada).
    fn choose_var(&mut self, pref: Ty) -> (String, Ty, bool) {
        let name = VAR_NAMES[self.rng.below(VAR_NAMES.len() as u32) as usize].to_string();
        match self.vars.iter().find(|(n, _)| *n == name) {
            Some((_, t)) => (name, *t, false),
            None => (name, pref, true),
        }
    }

    // ── Bloque de sentencias ───────────────────────────────────────────────
    fn block(&mut self, depth: u32, n: u32) {
        for _ in 0..n {
            self.stmt(depth);
        }
    }

    /// Bloque anidado con scope propio: las variables definidas dentro NO
    /// escapan al bloque exterior. Evita referenciar después una variable que
    /// solo existiría si una rama `if` (que pudo no ejecutarse) hubiera corrido.
    fn scoped_block(&mut self, depth: u32, n: u32) {
        let saved = self.vars.len();
        self.block(depth, n);
        self.vars.truncate(saved);
    }

    fn stmt(&mut self, depth: u32) {
        // En profundidad 0 solo asignaciones/show (sin más anidamiento).
        // Depth 0: assignment, show or a collection operation (no nesting).
        let kind = if depth == 0 {
            [0, 1, 5][self.rng.below(3) as usize]
        } else {
            self.rng.below(7)
        };
        match kind {
            0 => {
                // La var conserva su tipo si ya existía. El RHS se genera ANTES
                // de registrar la var nueva (evita use-before-def).
                let pref = match self.rng.below(20) {
                    0..=7 => Ty::Num,
                    8..=10 => Ty::Str,
                    11..=13 => Ty::Dec,
                    14..=16 => Ty::List,
                    _ => Ty::Dict,
                };
                let (v, ty, is_new) = self.choose_var(pref);
                let e = self.expr_of(ty, 3);
                if is_new { self.vars.push((v.clone(), ty)); }
                self.line(&format!("{} = {}", v, e));
            }
            1 => {
                let e = match self.rng.below(10) {
                    0..=2 => self.str_expr(3),
                    3..=4 => self.dec_expr(3),
                    5 => self.list_expr(2),
                    _ => self.num_expr(3),
                };
                if self.rng.chance(1, 3) { self.attempt_show(&e); } else { self.line(&format!("show {}", e)); }
            }
            5 | 6 => self.collection_stmt(),
            2 => {
                // if / else
                let cond = self.bool_expr(2);
                self.line(&format!("if {} {{", cond));
                self.indent += 1;
                let k1 = 1 + self.rng.below(3);
                self.scoped_block(depth - 1, k1);
                self.indent -= 1;
                if self.rng.chance(1, 2) {
                    self.line("} else {");
                    self.indent += 1;
                    let k2 = 1 + self.rng.below(3);
                    self.scoped_block(depth - 1, k2);
                    self.indent -= 1;
                }
                self.line("}");
            }
            3 => {
                // while con contador acotado → termina siempre.
                let counter = format!("i{}", self.rng.below(1000));
                let bound = 1 + self.rng.below(5);
                self.line(&format!("{} = 0", counter));
                self.line(&format!("while {} < {} {{", counter, bound));
                self.indent += 1;
                // Cuerpo: sentencias que NO tocan el contador.
                let k = 1 + self.rng.below(2);
                self.scoped_block(depth - 1, k);
                self.line(&format!("{} = {} + 1", counter, counter));
                self.indent -= 1;
                self.line("}");
            }
            4 => {
                // show de un booleano (imprime yes/no).
                let e = self.bool_expr(2);
                self.line(&format!("show {}", e));
            }
            _ => unreachable!(),
        }
    }

    /// Genera el programa completo y devuelve el texto fuente.
    fn program(&mut self) -> String {
        // Shape used by the instance statements.
        self.line("shape P {");
        self.line("    x");
        self.line("}");
        // 0-2 funciones int→int no recursivas (terminación trivial).
        let nfns = self.rng.below(3);
        for k in 0..nfns {
            let fname = format!("fn{}", k);
            // Funciones num→num: el parámetro `x` entra como variable numérica en
            // scope SOLO del cuerpo (los call-sites pasan siempre un num_expr).
            self.vars.push(("x".to_string(), Ty::Num));
            self.line(&format!("fn {}(x) {{", fname));
            self.indent += 1;
            let e = self.num_expr(3);
            self.line(&format!("return {}", e));
            self.indent -= 1;
            self.line("}");
            self.vars.retain(|(n, _)| n != "x");
            self.funcs.push(fname);
        }
        // Cuerpo principal.
        let nstmts = 4 + self.rng.below(8);
        self.block(2, nstmts);
        std::mem::take(&mut self.out)
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Ejecución y comparación.
// ─────────────────────────────────────────────────────────────────────────────
fn write_temp(src: &str) -> std::path::PathBuf {
    let id = COUNTER.fetch_add(1, Ordering::SeqCst);
    let mut p = std::env::temp_dir();
    p.push(format!("orion_fuzz_{}_{}.orx", std::process::id(), id));
    fs::write(&p, src).expect("escribir temporal");
    p
}

fn run(args: &[&str]) -> (String, String, bool) {
    let out = Command::new(env!("CARGO_BIN_EXE_orion"))
        .args(args)
        .output()
        .expect("ejecutar binario orion");
    (
        String::from_utf8_lossy(&out.stdout).into_owned(),
        String::from_utf8_lossy(&out.stderr).into_owned(),
        out.status.success(),
    )
}

/// Ejecuta `src` por VM y por JIT; `Err(detalle)` si divergen o si el JIT no
/// compiló (eso sería VM contra VM). `Ok(true)` si el programa terminó bien.
fn check_agreement(src: &str) -> Result<bool, String> {
    let path = write_temp(src);
    let p = path.to_str().unwrap();
    let (vm_out, _, vm_ok) = run(&[p]);
    let (jit_out, jit_err, jit_ok) = run(&["--jit", p]);
    let _ = fs::remove_file(&path);

    if let Some(line) = jit_err.lines().find(|l| l.contains("falling back")) {
        return Err(format!("the JIT did not compile the program: {line}"));
    }

    if vm_ok != jit_ok {
        return Err(format!(
            "DISCREPANCIA éxito/fallo: vm_ok={vm_ok} jit_ok={jit_ok}\n\
             --- VM stdout ---\n{vm_out}--- JIT stdout ---\n{jit_out}"
        ));
    }
    if vm_out != jit_out {
        return Err(format!(
            "DISCREPANCIA stdout:\n--- VM ---\n{vm_out}--- JIT ---\n{jit_out}"
        ));
    }
    Ok(vm_ok)
}

fn fuzz_count() -> u64 {
    // Default modesto para mantener `cargo test` ágil (cada programa lanza 2
    // procesos). Para una corrida profunda: ORION_FUZZ_N=5000 cargo test ...
    std::env::var("ORION_FUZZ_N").ok().and_then(|s| s.parse().ok()).unwrap_or(60)
}

/// Why generated programs stop early: counts the runtime error messages (digits
/// masked) over the same seeds as the fuzz run, and prints two failing samples.
#[test]
#[ignore]
fn dump_samples() {
    let n = fuzz_count();
    let mut reasons: std::collections::BTreeMap<String, u32> = Default::default();
    let mut shown = 0;
    for i in 0..n {
        let seed = 0xC0FFEEu64.wrapping_add(i).wrapping_mul(0x100000001B3);
        let src = Gen::new(seed).program();
        let path = write_temp(&src);
        let (_, err, ok) = run(&[path.to_str().unwrap()]);
        let _ = fs::remove_file(&path);
        if ok { continue; }
        // The message is the line after the "runtime error" header, without ANSI codes.
        let plain: String = strip_ansi(&err);
        let mut lines = plain.lines().map(str::trim).filter(|l| !l.is_empty());
        let msg = lines.by_ref().find(|l| l.contains(" error")).and_then(|_| lines.next()).unwrap_or("?");
        let key: String = msg.chars().map(|c| if c.is_ascii_digit() { '#' } else { c }).collect();
        *reasons.entry(key).or_default() += 1;
        if shown < 2 {
            shown += 1;
            eprintln!("===== sample (seed {seed:#x}) =====\n{src}--- {msg}");
        }
    }
    let mut sorted: Vec<_> = reasons.into_iter().collect();
    sorted.sort_by(|a, b| b.1.cmp(&a.1));
    eprintln!("===== why {n} programs stopped =====");
    for (msg, count) in sorted {
        eprintln!("{count:4}  {msg}");
    }
}

fn strip_ansi(s: &str) -> String {
    let mut out = String::new();
    let mut chars = s.chars();
    while let Some(c) = chars.next() {
        if c == '\x1b' {
            for c2 in chars.by_ref() { if c2.is_ascii_alphabetic() { break; } }
        } else {
            out.push(c);
        }
    }
    out
}

/// Meta-test: el generador debe producir programas SINTÁCTICAMENTE VÁLIDOS.
///
/// Un fallo de runtime (overflow, módulo por cero) es legítimo: el programa
/// ejecutó hasta ese punto y ambos backends concuerdan en el error — eso SÍ
/// prueba paridad. Lo que invalidaría el fuzzer es generar basura que no
/// parsea (entonces ambos backends "fallan igual" sin ejecutar nada). Esta red
/// detecta esa regresión del propio generador.
#[test]
fn fuzz_generator_produces_valid_syntax() {
    let mut syntax_errors = 0;
    let total = 60;
    for i in 0..total {
        let seed = (0xBEEFu64).wrapping_add(i).wrapping_mul(0x100000001B3);
        let src = Gen::new(seed).program();
        let path = write_temp(&src);
        let out = Command::new(env!("CARGO_BIN_EXE_orion"))
            .arg(path.to_str().unwrap())
            .output()
            .unwrap();
        let _ = fs::remove_file(&path);
        let stderr = String::from_utf8_lossy(&out.stderr);
        // Any of these means the program never ran (the old check looked for the
        // Spanish messages, so it could no longer fail).
        let never_ran = ["syntax error", "lexical error", "type error", "compile error", "ParseInt"];
        if never_ran.iter().any(|m| stderr.contains(m)) {
            syntax_errors += 1;
            eprintln!("✗ Sintaxis inválida (semilla {seed:#x}):\n{src}\n--- stderr ---\n{stderr}");
        }
    }
    assert_eq!(
        syntax_errors, 0,
        "{syntax_errors}/{total} programas generados tienen error de sintaxis → bug del generador"
    );
}

#[test]
fn differential_fuzz_vm_vs_jit() {
    let base_seed: u64 = std::env::var("ORION_FUZZ_SEED")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(0xC0FFEE);
    let n = fuzz_count();
    let mut completed: u64 = 0;

    for i in 0..n {
        let seed = base_seed.wrapping_add(i).wrapping_mul(0x100000001B3);
        let src = Gen::new(seed).program();
        match check_agreement(&src) {
            Ok(ok) => if ok { completed += 1 },
            Err(detail) => panic!(
                "\n╳ Divergencia VM↔JIT en iteración {i} (semilla {seed:#x}).\n\
                 Reproduce con: ORION_FUZZ_SEED={base_seed} ORION_FUZZ_N={} (iter {i})\n\
                 {detail}\n--- PROGRAMA ---\n{src}",
                n
            ),
        }
    }
    // Programs that die on their first lines test little: keep most of them running.
    eprintln!("fuzz: {completed}/{n} programs ran to the end");
    if n >= 20 {
        assert!(completed * 100 / n >= 30, "only {completed}/{n} programs ran to the end: the generator needs tuning");
    }
}
