/// Orion Cosmos — simulación gravitacional en Rust puro.
/// Cuerpos como Dicts, universo como Dict con lista de cuerpos.
///
/// Ninguna constante de la simulación está fijada: G, el softening y los rangos
/// de generación se pasan en un Dict de opciones. Los valores por defecto son
/// los del SI en el vacío, pero nada impide simular otras escalas o unidades.
use crate::eval_value::EvalValue;
use indexmap::IndexMap as HashMap;
use rand::{Rng, SeedableRng};
use rand::rngs::StdRng;

/// Constante de gravitación universal en unidades SI (m³·kg⁻¹·s⁻²).
/// Solo es el valor por defecto: `opts["g"]` lo reemplaza.
const G_SI: f64 = 6.674e-11;

/// Parámetros de la física, leídos de las opciones.
#[derive(Clone, Copy)]
struct Physics {
    /// Constante de gravitación.
    g: f64,
    /// Softening de Plummer: F = G·m₁·m₂ / (r² + ε²). Evita que la fuerza
    /// diverja cuando dos cuerpos se acercan, sin descartar la interacción.
    softening: f64,
}

impl Default for Physics {
    fn default() -> Self {
        Physics { g: G_SI, softening: 0.0 }
    }
}

pub fn call(function: &str, args: Vec<EvalValue>) -> Result<EvalValue, String> {
    match function {
        // body(name, mass, x, y, z, vx, vy, vz) → dict body
        "body" => {
            let name = if args.is_empty() { "body".into() } else { to_str(&args[0]) };
            let mass = if args.len() > 1 { to_f64(&args[1])? } else { 1.0e24 };
            let x  = if args.len() > 2 { to_f64(&args[2])? } else { 0.0 };
            let y  = if args.len() > 3 { to_f64(&args[3])? } else { 0.0 };
            let z  = if args.len() > 4 { to_f64(&args[4])? } else { 0.0 };
            let vx = if args.len() > 5 { to_f64(&args[5])? } else { 0.0 };
            let vy = if args.len() > 6 { to_f64(&args[6])? } else { 0.0 };
            let vz = if args.len() > 7 { to_f64(&args[7])? } else { 0.0 };
            Ok(make_body(&name, mass, [x,y,z], [vx,vy,vz]))
        }
        // random_star(opts?) → body con masa y posición aleatorias
        // opts = { mass_min, mass_max, pos_min, pos_max, vel_min, vel_max, seed }
        "random_star" | "star" => {
            let r = SpawnRange::from(args.first())?;
            let mut rng = r.rng();
            let name = format!("Star_{}", rng.gen_range(1000..9999));
            Ok(spawn_body(&name, &r, &mut *rng))
        }
        // create(n?, opts?) → universo con n cuerpos aleatorios
        // opts = { mass_min, mass_max, pos_min, pos_max, vel_min, vel_max, seed }
        "create" | "universe" => {
            let n = if args.is_empty() { 5 } else { to_i64(&args[0])? as usize };
            let r = SpawnRange::from(args.get(1))?;
            let mut rng = r.rng();
            let bodies: Vec<EvalValue> = (0..n)
                .map(|i| spawn_body(&format!("Star_{}", i + 1), &r, &mut *rng))
                .collect();
            let mut m = HashMap::new();
            m.insert("bodies".into(), EvalValue::List(bodies));
            m.insert("time".into(),   EvalValue::Float(0.0));
            Ok(EvalValue::Dict(m))
        }
        // step(universe, dt?, opts?) → universo actualizado
        // opts = { g, softening }
        "step" => {
            if args.is_empty() { return Err("cosmos.step requiere (universe, dt?, opts?)".into()); }
            let dt = if args.len() > 1 { to_f64(&args[1])? } else { 1.0 };
            let phys = physics_from(args.get(2))?;
            step_universe(args[0].clone(), dt, phys)
        }
        // run(universe, steps?, dt?, opts?) → universo final
        "run" => {
            if args.is_empty() { return Err("cosmos.run requiere (universe, steps?, dt?, opts?)".into()); }
            let mut universe = args[0].clone();
            let steps = if args.len() > 1 { to_i64(&args[1])? as usize } else { 10 };
            let dt    = if args.len() > 2 { to_f64(&args[2])? } else { 1.0 };
            let phys  = physics_from(args.get(3))?;
            for _ in 0..steps {
                universe = step_universe(universe, dt, phys)?;
            }
            Ok(universe)
        }
        // summary(universe) → {time, bodies}
        "summary" => {
            if args.is_empty() { return Err("cosmos.summary requiere (universe)".into()); }
            universe_summary(&args[0])
        }
        // gravity(b1, b2, G|opts?) → fuerza [fx, fy, fz]
        // El tercer argumento admite un número (G) o un Dict { g, softening }.
        "gravity" => {
            if args.len() < 2 { return Err("cosmos.gravity requiere (b1, b2, G|opts?)".into()); }
            let phys = physics_from(args.get(2))?;
            let b1 = parse_body(&args[0])?;
            let b2 = parse_body(&args[1])?;
            let force = compute_gravity(&b1, &b2, phys);
            Ok(EvalValue::List(force.iter().map(|&f| EvalValue::Float(f)).collect()))
        }
        // energy(universe, G|opts?) → {kinetic, potential, total}
        "energy" => {
            if args.is_empty() { return Err("cosmos.energy requiere (universe, G|opts?)".into()); }
            let phys = physics_from(args.get(1))?;
            universe_energy(&args[0], phys)
        }
        // distance(b1, b2) → f64
        "distance" => {
            if args.len() < 2 { return Err("cosmos.distance requiere (b1, b2)".into()); }
            let b1 = parse_body(&args[0])?;
            let b2 = parse_body(&args[1])?;
            let d  = body_distance(&b1, &b2);
            Ok(EvalValue::Float(d))
        }
        // stardust(n?, opts?) → lista de n puntos 3D aleatorios
        // opts = { min, max, seed }
        "stardust" | "dust" => {
            let n = if args.is_empty() { 100 } else { to_i64(&args[0])? as usize };
            let (mut lo, mut hi, mut seed) = (-1.0f64, 1.0f64, None);
            if let Some(EvalValue::Dict(m)) = args.get(1) {
                if let Some(v) = opt_f64(m, &["min", "minimo"]) { lo = v; }
                if let Some(v) = opt_f64(m, &["max", "maximo"]) { hi = v; }
                seed = opt_u64(m, &["seed", "semilla"]);
                if lo > hi {
                    return Err(format!("cosmos.stardust: rango inválido ({lo} > {hi})"));
                }
            }
            let mut rng: Box<dyn RngCore> = match seed {
                Some(s) => Box::new(StdRng::seed_from_u64(s)),
                None    => Box::new(rand::thread_rng()),
            };
            let dust: Vec<EvalValue> = (0..n).map(|_| {
                EvalValue::List((0..3)
                    .map(|_| EvalValue::Float(SpawnRange::sample(&mut *rng, (lo, hi))))
                    .collect())
            }).collect();
            Ok(EvalValue::List(dust))
        }

        f => Err(format!("cosmos.{}() no existe", f)),
    }
}

//     Cuerpo                                                                    

struct Body { name: String, mass: f64, pos: [f64;3], vel: [f64;3] }

fn make_body(name: &str, mass: f64, pos: [f64;3], vel: [f64;3]) -> EvalValue {
    let mut m = HashMap::new();
    m.insert("name".into(), EvalValue::Str(name.to_string()));
    m.insert("mass".into(), EvalValue::Float(mass));
    m.insert("x".into(),  EvalValue::Float(pos[0]));
    m.insert("y".into(),  EvalValue::Float(pos[1]));
    m.insert("z".into(),  EvalValue::Float(pos[2]));
    m.insert("vx".into(), EvalValue::Float(vel[0]));
    m.insert("vy".into(), EvalValue::Float(vel[1]));
    m.insert("vz".into(), EvalValue::Float(vel[2]));
    EvalValue::Dict(m)
}

fn parse_body(v: &EvalValue) -> Result<Body, String> {
    let EvalValue::Dict(m) = v else { return Err("cosmos: se esperaba un body (dict)".into()); };
    Ok(Body {
        name: m.get("name").map(|x| format!("{}", x)).unwrap_or_default(),
        mass: to_f64(m.get("mass").ok_or("cosmos: body sin campo 'mass'")?)?,
        pos: [
            to_f64(m.get("x").unwrap_or(&EvalValue::Float(0.0)))?,
            to_f64(m.get("y").unwrap_or(&EvalValue::Float(0.0)))?,
            to_f64(m.get("z").unwrap_or(&EvalValue::Float(0.0)))?,
        ],
        vel: [
            to_f64(m.get("vx").unwrap_or(&EvalValue::Float(0.0)))?,
            to_f64(m.get("vy").unwrap_or(&EvalValue::Float(0.0)))?,
            to_f64(m.get("vz").unwrap_or(&EvalValue::Float(0.0)))?,
        ],
    })
}

fn body_to_eval(b: &Body) -> EvalValue {
    make_body(&b.name, b.mass, b.pos, b.vel)
}

//     Opciones

/// Lee un número del Dict aceptando cualquiera de los alias dados.
fn opt_f64(m: &HashMap<String, EvalValue>, keys: &[&str]) -> Option<f64> {
    for k in keys {
        if let Some(v) = m.get(*k) {
            if let Ok(f) = to_f64(v) { return Some(f); }
        }
    }
    None
}

fn opt_u64(m: &HashMap<String, EvalValue>, keys: &[&str]) -> Option<u64> {
    for k in keys {
        if let Some(v) = m.get(*k) {
            if let Ok(n) = to_i64(v) { return Some(n as u64); }
        }
    }
    None
}

/// Física a partir de un argumento opcional, que puede ser:
///   - un número → se interpreta como G (forma corta e histórica de `gravity`)
///   - un Dict   → `{ "g": ..., "softening": ... }`, con alias en español
fn physics_from(arg: Option<&EvalValue>) -> Result<Physics, String> {
    let mut p = Physics::default();
    match arg {
        None | Some(EvalValue::Null) => {}
        Some(EvalValue::Dict(m)) => {
            if let Some(g) = opt_f64(m, &["g", "G", "gravedad", "gravity"]) { p.g = g; }
            if let Some(s) = opt_f64(m, &["softening", "suavizado", "epsilon"]) {
                if s < 0.0 { return Err("cosmos: 'softening' no puede ser negativo".into()); }
                p.softening = s;
            }
        }
        Some(other) => p.g = to_f64(other)?,
    }
    Ok(p)
}

/// Rangos de generación aleatoria. Sin opciones usa una escala estelar
/// arbitraria pero razonable; con ellas, la que el developer necesite.
struct SpawnRange {
    mass: (f64, f64),
    pos:  (f64, f64),
    vel:  (f64, f64),
    seed: Option<u64>,
}

impl Default for SpawnRange {
    fn default() -> Self {
        SpawnRange {
            mass: (1e20, 1e30),
            pos:  (-1e5, 1e5),
            vel:  (-10.0, 10.0),
            seed: None,
        }
    }
}

impl SpawnRange {
    /// `{ mass_min, mass_max, pos_min, pos_max, vel_min, vel_max, seed }`
    fn from(arg: Option<&EvalValue>) -> Result<Self, String> {
        let mut r = SpawnRange::default();
        let Some(EvalValue::Dict(m)) = arg else { return Ok(r) };

        let pairs: [(&mut (f64, f64), [&[&str]; 2]); 3] = [
            (&mut r.mass, [&["mass_min", "masa_min"], &["mass_max", "masa_max"]]),
            (&mut r.pos,  [&["pos_min"],              &["pos_max"]]),
            (&mut r.vel,  [&["vel_min"],              &["vel_max"]]),
        ];
        for (target, keys) in pairs {
            if let Some(v) = opt_f64(m, keys[0]) { target.0 = v; }
            if let Some(v) = opt_f64(m, keys[1]) { target.1 = v; }
            if target.0 > target.1 {
                return Err(format!(
                    "cosmos: rango inválido ({} > {}): el mínimo no puede superar al máximo",
                    target.0, target.1
                ));
            }
        }
        r.seed = opt_u64(m, &["seed", "semilla"]);
        Ok(r)
    }

    /// RNG sembrado si el developer pidió reproducibilidad, aleatorio si no.
    fn rng(&self) -> Box<dyn RngCore> {
        match self.seed {
            Some(s) => Box::new(StdRng::seed_from_u64(s)),
            None    => Box::new(rand::thread_rng()),
        }
    }

    /// Un rango degenerado (min == max) haría entrar en pánico a `gen_range`.
    fn sample(rng: &mut dyn RngCore, (lo, hi): (f64, f64)) -> f64 {
        if lo >= hi { lo } else { rng.gen_range(lo..hi) }
    }
}

use rand::RngCore;

//     Física

fn body_distance(b1: &Body, b2: &Body) -> f64 {
    let d: f64 = (0..3).map(|i| (b1.pos[i] - b2.pos[i]).powi(2)).sum();
    d.sqrt()
}

fn compute_gravity(b1: &Body, b2: &Body, p: Physics) -> [f64; 3] {
    let dist = body_distance(b1, b2);
    // Con softening = 0 se conserva el comportamiento newtoniano exacto y solo
    // se descarta el caso degenerado de dos cuerpos en la misma posición.
    let denom = dist * dist + p.softening * p.softening;
    if denom <= 0.0 { return [0.0; 3]; }
    if p.softening == 0.0 && dist == 0.0 { return [0.0; 3]; }
    let f = p.g * b1.mass * b2.mass / denom;
    let mut force = [0.0f64; 3];
    // La dirección se normaliza con la distancia real, no con la suavizada.
    let dir_norm = if dist > 0.0 { dist } else { 1.0 };
    for i in 0..3 { force[i] = f * (b2.pos[i] - b1.pos[i]) / dir_norm; }
    force
}

/// Cuerpo aleatorio dentro de los rangos pedidos.
fn spawn_body(name: &str, r: &SpawnRange, rng: &mut dyn RngCore) -> EvalValue {
    let mass = SpawnRange::sample(rng, r.mass);
    let mut pos = [0.0f64; 3];
    let mut vel = [0.0f64; 3];
    for i in 0..3 {
        pos[i] = SpawnRange::sample(rng, r.pos);
        vel[i] = SpawnRange::sample(rng, r.vel);
    }
    make_body(name, mass, pos, vel)
}

fn step_universe(universe: EvalValue, dt: f64, phys: Physics) -> Result<EvalValue, String> {
    let EvalValue::Dict(mut uni_map) = universe else {
        return Err("cosmos.step: se esperaba un universo (dict)".into());
    };
    let bodies_val = uni_map.get("bodies").cloned().ok_or("cosmos.step: universo sin 'bodies'")?;
    let EvalValue::List(body_vals) = bodies_val else {
        return Err("cosmos.step: 'bodies' debe ser una lista".into());
    };

    let mut bodies: Vec<Body> = body_vals.iter().map(parse_body).collect::<Result<_, _>>()?;
    let n = bodies.len();

    // Calcular fuerzas
    let mut forces = vec![[0.0f64; 3]; n];
    for i in 0..n {
        for j in (i+1)..n {
            let f = compute_gravity(&bodies[i], &bodies[j], phys);
            for k in 0..3 {
                forces[i][k] += f[k];
                forces[j][k] -= f[k];
            }
        }
    }

    // Actualizar velocidades y posiciones
    for i in 0..n {
        for k in 0..3 {
            bodies[i].vel[k] += forces[i][k] / bodies[i].mass * dt;
            bodies[i].pos[k] += bodies[i].vel[k] * dt;
        }
    }

    let time = to_f64(uni_map.get("time").unwrap_or(&EvalValue::Float(0.0)))? + dt;
    uni_map.insert("bodies".into(), EvalValue::List(bodies.iter().map(body_to_eval).collect()));
    uni_map.insert("time".into(), EvalValue::Float(time));
    Ok(EvalValue::Dict(uni_map))
}

fn universe_summary(universe: &EvalValue) -> Result<EvalValue, String> {
    let EvalValue::Dict(m) = universe else { return Err("cosmos.summary: se esperaba un universo".into()); };
    let count = match m.get("bodies") {
        Some(EvalValue::List(v)) => v.len() as i64,
        _ => 0,
    };
    let time = to_f64(m.get("time").unwrap_or(&EvalValue::Float(0.0)))?;
    let mut result = HashMap::new();
    result.insert("time".into(),    EvalValue::Float(time));
    result.insert("bodies".into(),  EvalValue::Int(count));
    Ok(EvalValue::Dict(result))
}

fn universe_energy(universe: &EvalValue, phys: Physics) -> Result<EvalValue, String> {
    let EvalValue::Dict(m) = universe else { return Err("cosmos.energy: se esperaba un universo".into()); };
    let bodies_val = m.get("bodies").ok_or("cosmos.energy: universo sin 'bodies'")?;
    let EvalValue::List(body_vals) = bodies_val else { return Err("cosmos.energy: 'bodies' debe ser lista".into()); };
    let bodies: Vec<Body> = body_vals.iter().map(parse_body).collect::<Result<_, _>>()?;

    let kinetic: f64 = bodies.iter().map(|b| {
        0.5 * b.mass * b.vel.iter().map(|v| v * v).sum::<f64>()
    }).sum();

    // El potencial usa el mismo softening que la fuerza; si no, la energía no
    // se conserva en las simulaciones que lo activan.
    let mut potential = 0.0f64;
    for i in 0..bodies.len() {
        for j in (i+1)..bodies.len() {
            let r = body_distance(&bodies[i], &bodies[j]);
            let denom = (r * r + phys.softening * phys.softening).sqrt();
            if denom > 0.0 {
                potential -= phys.g * bodies[i].mass * bodies[j].mass / denom;
            }
        }
    }

    let mut res = HashMap::new();
    res.insert("kinetic".into(),   EvalValue::Float(kinetic));
    res.insert("potential".into(), EvalValue::Float(potential));
    res.insert("total".into(),     EvalValue::Float(kinetic + potential));
    Ok(EvalValue::Dict(res))
}

//     Helpers                                                                   

fn to_f64(v: &EvalValue) -> Result<f64, String> {
    match v {
        EvalValue::Float(f) => Ok(*f),
        EvalValue::Int(n)   => Ok(*n as f64),
        other => Err(format!("cosmos: esperaba número, recibió {}", other.type_name())),
    }
}

fn to_i64(v: &EvalValue) -> Result<i64, String> {
    match v {
        EvalValue::Int(n)   => Ok(*n),
        EvalValue::Float(f) => Ok(*f as i64),
        other => Err(format!("cosmos: esperaba entero, recibió {}", other.type_name())),
    }
}

fn to_str(v: &EvalValue) -> String {
    match v { EvalValue::Str(s) => s.clone(), other => format!("{}", other) }
}
