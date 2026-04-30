use crate::eval_value::EvalValue;

// Tipo interno: matriz de f64
type Mat = Vec<Vec<f64>>;

pub fn call(function: &str, args: Vec<EvalValue>) -> Result<EvalValue, String> {
    match function {
        // add(A, B) → A + B
        "add" => {
            if args.len() < 2 { return Err("matrix.add requiere (A, B)".into()); }
            let a = parse_mat(&args[0])?;
            let b = parse_mat(&args[1])?;
            Ok(mat_to_eval(mat_add(&a, &b)?))
        }
        // sub(A, B) → A - B
        "sub" => {
            if args.len() < 2 { return Err("matrix.sub requiere (A, B)".into()); }
            let a = parse_mat(&args[0])?;
            let b = parse_mat(&args[1])?;
            Ok(mat_to_eval(mat_sub(&a, &b)?))
        }
        // mul(A, B) → A @ B  (soporta escalar × matriz)
        "mul" => {
            if args.len() < 2 { return Err("matrix.mul requiere (A, B)".into()); }
            match (&args[0], &args[1]) {
                (EvalValue::Float(s), _) => {
                    let b = parse_mat(&args[1])?;
                    Ok(mat_to_eval(scalar_mul(*s, &b)))
                }
                (EvalValue::Int(s), _) => {
                    let b = parse_mat(&args[1])?;
                    Ok(mat_to_eval(scalar_mul(*s as f64, &b)))
                }
                (_, EvalValue::Float(s)) => {
                    let a = parse_mat(&args[0])?;
                    Ok(mat_to_eval(scalar_mul(*s, &a)))
                }
                (_, EvalValue::Int(s)) => {
                    let a = parse_mat(&args[0])?;
                    Ok(mat_to_eval(scalar_mul(*s as f64, &a)))
                }
                _ => {
                    let a = parse_mat(&args[0])?;
                    let b = parse_mat(&args[1])?;
                    Ok(mat_to_eval(mat_mul(&a, &b)?))
                }
            }
        }
        // transpose(A) → A^T
        "transpose" => {
            let a = parse_mat(&args[0])?;
            Ok(mat_to_eval(transpose(&a)))
        }
        // det(A) → determinante (f64)
        "det" => {
            let a = parse_mat(&args[0])?;
            Ok(EvalValue::Float(det(&a)?))
        }
        // inverse(A) → A^-1
        "inverse" => {
            let a = parse_mat(&args[0])?;
            Ok(mat_to_eval(inverse(&a)?))
        }
        // trace(A) → suma de la diagonal
        "trace" => {
            let a = parse_mat(&args[0])?;
            let t: f64 = (0..a.len().min(a[0].len())).map(|i| a[i][i]).sum();
            Ok(EvalValue::Float(t))
        }
        // identity(n) → matriz identidad n×n
        "identity" => {
            let n = to_i64(args.first().ok_or("matrix.identity requiere (n)")?)? as usize;
            Ok(mat_to_eval(identity(n)))
        }
        // zeros(rows, cols) → matriz de ceros
        "zeros" => {
            if args.len() < 2 { return Err("matrix.zeros requiere (rows, cols)".into()); }
            let r = to_i64(&args[0])? as usize;
            let c = to_i64(&args[1])? as usize;
            Ok(mat_to_eval(vec![vec![0.0; c]; r]))
        }
        // ones(rows, cols) → matriz de unos
        "ones" => {
            if args.len() < 2 { return Err("matrix.ones requiere (rows, cols)".into()); }
            let r = to_i64(&args[0])? as usize;
            let c = to_i64(&args[1])? as usize;
            Ok(mat_to_eval(vec![vec![1.0; c]; r]))
        }
        // shape(A) → [rows, cols]
        "shape" => {
            let a = parse_mat(&args[0])?;
            let rows = a.len() as i64;
            let cols = a.first().map(|r| r.len()).unwrap_or(0) as i64;
            Ok(EvalValue::List(vec![EvalValue::Int(rows), EvalValue::Int(cols)]))
        }
        // dot(A, B) → producto punto (misma que mul)
        "dot" => {
            if args.len() < 2 { return Err("matrix.dot requiere (A, B)".into()); }
            let a = parse_mat(&args[0])?;
            let b = parse_mat(&args[1])?;
            Ok(mat_to_eval(mat_mul(&a, &b)?))
        }
        // rot2D(angle_deg) → matriz de rotación 2D
        "rot2D" => {
            let deg = to_f64(args.first().ok_or("matrix.rot2D requiere (angle_deg)")?)?;
            let a = deg.to_radians();
            Ok(mat_to_eval(vec![
                vec![a.cos(), -a.sin()],
                vec![a.sin(),  a.cos()],
            ]))
        }
        // neuralify(A, activation?) → aplica función de activación
        "neuralify" => {
            if args.is_empty() { return Err("matrix.neuralify requiere (A, activation?)".into()); }
            let a   = parse_mat(&args[0])?;
            let act = if args.len() > 1 { to_str(&args[1]) } else { "relu".into() };
            let result = a.into_iter().map(|row| {
                row.into_iter().map(|x| match act.as_str() {
                    "relu"    => x.max(0.0),
                    "sigmoid" => 1.0 / (1.0 + (-x).exp()),
                    "tanh"    => x.tanh(),
                    _         => x,
                }).collect()
            }).collect();
            Ok(mat_to_eval(result))
        }
        // flatten(A) → lista 1D
        "flatten" => {
            let a = parse_mat(&args[0])?;
            let flat: Vec<EvalValue> = a.into_iter().flatten().map(EvalValue::Float).collect();
            Ok(EvalValue::List(flat))
        }
        // scale(A, factor) → multiplica todos los elementos
        "scale" | "amplify" => {
            if args.len() < 2 { return Err("matrix.scale requiere (A, factor)".into()); }
            let a      = parse_mat(&args[0])?;
            let factor = to_f64(&args[1])?;
            Ok(mat_to_eval(scalar_mul(factor, &a)))
        }
        // collapse(A) → tanh(sum de todos los elementos)
        "collapse" => {
            let a = parse_mat(&args[0])?;
            let flat: f64 = a.iter().flatten().sum();
            Ok(EvalValue::Float(flat.tanh()))
        }

        f => Err(format!("matrix.{}() no existe", f)),
    }
}

// ─── Operaciones matriciales ──────────────────────────────────────────────────

fn mat_add(a: &Mat, b: &Mat) -> Result<Mat, String> {
    check_same_shape(a, b)?;
    Ok(a.iter().zip(b).map(|(ra, rb)| ra.iter().zip(rb).map(|(x, y)| x + y).collect()).collect())
}

fn mat_sub(a: &Mat, b: &Mat) -> Result<Mat, String> {
    check_same_shape(a, b)?;
    Ok(a.iter().zip(b).map(|(ra, rb)| ra.iter().zip(rb).map(|(x, y)| x - y).collect()).collect())
}

fn mat_mul(a: &Mat, b: &Mat) -> Result<Mat, String> {
    let (r1, c1) = shape(a);
    let (r2, c2) = shape(b);
    if c1 != r2 { return Err(format!("matrix.mul: dimensiones incompatibles ({}x{}) × ({}x{})", r1, c1, r2, c2)); }
    let mut result = vec![vec![0.0; c2]; r1];
    for i in 0..r1 {
        for j in 0..c2 {
            result[i][j] = (0..c1).map(|k| a[i][k] * b[k][j]).sum();
        }
    }
    Ok(result)
}

fn scalar_mul(s: f64, a: &Mat) -> Mat {
    a.iter().map(|row| row.iter().map(|x| x * s).collect()).collect()
}

fn transpose(a: &Mat) -> Mat {
    let (r, c) = shape(a);
    (0..c).map(|j| (0..r).map(|i| a[i][j]).collect()).collect()
}

fn det(a: &Mat) -> Result<f64, String> {
    let (r, c) = shape(a);
    if r != c { return Err("matrix.det: la matriz debe ser cuadrada".into()); }
    if r == 1 { return Ok(a[0][0]); }
    if r == 2 { return Ok(a[0][0]*a[1][1] - a[0][1]*a[1][0]); }
    let mut total = 0.0f64;
    for col in 0..r {
        let minor: Mat = a[1..].iter().map(|row| {
            row.iter().enumerate().filter(|(j, _)| *j != col).map(|(_, v)| *v).collect()
        }).collect();
        total += (if col % 2 == 0 { 1.0 } else { -1.0 }) * a[0][col] * det(&minor)?;
    }
    Ok(total)
}

fn inverse(a: &Mat) -> Result<Mat, String> {
    let n = a.len();
    if shape(a).0 != shape(a).1 { return Err("matrix.inverse: debe ser cuadrada".into()); }
    let id = identity(n);
    let mut m: Mat = a.iter().zip(&id).map(|(ar, ir)| ar.iter().chain(ir).copied().collect()).collect();
    for i in 0..n {
        let mut pivot = m[i][i];
        if pivot.abs() < 1e-12 {
            let swap = (i+1..n).find(|&j| m[j][i].abs() > 1e-12)
                .ok_or("matrix.inverse: matriz singular")?;
            m.swap(i, swap);
            pivot = m[i][i];
        }
        m[i] = m[i].iter().map(|x| x / pivot).collect();
        for j in 0..n {
            if j == i { continue; }
            let factor = m[j][i];
            let row_i = m[i].clone();
            for k in 0..2*n { m[j][k] -= factor * row_i[k]; }
        }
    }
    Ok(m.iter().map(|row| row[n..].to_vec()).collect())
}

fn identity(n: usize) -> Mat {
    let mut m = vec![vec![0.0; n]; n];
    for i in 0..n { m[i][i] = 1.0; }
    m
}

fn check_same_shape(a: &Mat, b: &Mat) -> Result<(), String> {
    let (r1, c1) = shape(a);
    let (r2, c2) = shape(b);
    if r1 != r2 || c1 != c2 {
        return Err(format!("matrix: dimensiones distintas ({}x{}) vs ({}x{})", r1, c1, r2, c2));
    }
    Ok(())
}

fn shape(m: &Mat) -> (usize, usize) {
    (m.len(), m.first().map(|r| r.len()).unwrap_or(0))
}

// ─── Conversiones EvalValue ↔ Mat ─────────────────────────────────────────────

fn parse_mat(v: &EvalValue) -> Result<Mat, String> {
    match v {
        EvalValue::List(rows) => {
            let mut mat = Vec::new();
            for row in rows {
                match row {
                    EvalValue::List(cols) => {
                        let nums: Result<Vec<f64>, _> = cols.iter().map(|x| to_f64(x)).collect();
                        mat.push(nums?);
                    }
                    EvalValue::Float(f) => mat.push(vec![*f]),
                    EvalValue::Int(n)   => mat.push(vec![*n as f64]),
                    _ => return Err("matrix: fila debe ser una lista de números".into()),
                }
            }
            if mat.is_empty() { return Err("matrix: lista vacía".into()); }
            Ok(mat)
        }
        EvalValue::Float(f) => Ok(vec![vec![*f]]),
        EvalValue::Int(n)   => Ok(vec![vec![*n as f64]]),
        _ => Err(format!("matrix: se esperaba lista, recibió {}", v.type_name())),
    }
}

fn mat_to_eval(m: Mat) -> EvalValue {
    EvalValue::List(m.into_iter().map(|row| {
        EvalValue::List(row.into_iter().map(|x| {
            if x.fract() == 0.0 { EvalValue::Int(x as i64) } else { EvalValue::Float(x) }
        }).collect())
    }).collect())
}

fn to_f64(v: &EvalValue) -> Result<f64, String> {
    match v {
        EvalValue::Float(f) => Ok(*f),
        EvalValue::Int(n)   => Ok(*n as f64),
        other => Err(format!("matrix: esperaba número, recibió {}", other.type_name())),
    }
}

fn to_i64(v: &EvalValue) -> Result<i64, String> {
    match v {
        EvalValue::Int(n)   => Ok(*n),
        EvalValue::Float(f) => Ok(*f as i64),
        other => Err(format!("matrix: esperaba entero, recibió {}", other.type_name())),
    }
}

fn to_str(v: &EvalValue) -> String {
    match v { EvalValue::Str(s) => s.clone(), other => format!("{}", other) }
}
