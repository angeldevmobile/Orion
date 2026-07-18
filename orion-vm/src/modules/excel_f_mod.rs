// excel.f — Builder de fórmulas vivas para excel.write_styled
//
// Uso:
//   f = excel.f
//   excel.write_styled("report.xlsx", data, {
//       formulas: {
//           "bonus":    f.pct("sales", 5),
//           "ratio":    f.ratio("sales", "target"),
//           "rank":     f.rank("sales", "desc"),
//           "total":    f.sum("sales"),
//           "acum":     f.cumulative("sales"),
//           "tier":     f.if_("sales", ">", 80000, "A", "B"),
//       }
//   })
//
// Cada función retorna un Dict descriptor { _f, col, ... } que write_styled
// convierte en fórmulas Excel vivas (se recalculan al abrir el archivo).

use crate::eval_value::EvalValue;
use indexmap::IndexMap as HashMap;

pub fn call(function: &str, args: Vec<EvalValue>) -> Result<EvalValue, String> {
    macro_rules! str_a {
        ($i:expr) => {
            match args.get($i) {
                Some(EvalValue::Str(s)) => s.clone(),
                Some(other)             => other.to_string(),
                None => return Err(format!(
                    "excel.f.{}: argumento {} requerido", function, $i + 1
                )),
            }
        };
    }
    macro_rules! num_a {
        ($i:expr) => {
            match args.get($i) {
                Some(EvalValue::Float(f)) => *f,
                Some(EvalValue::Int(i))   => *i as f64,
                _ => return Err(format!(
                    "excel.f.{}: argumento {} debe ser número", function, $i + 1
                )),
            }
        };
    }

    let mut d: HashMap<String, EvalValue> = HashMap::new();

    match function {
        // f.pct("col", percent) → =COL_ROW * (percent/100)
        // Ejemplo: f.pct("sales", 5) → =B6*0.05
        "pct" => {
            d.insert("_f".into(),  EvalValue::Str("pct".into()));
            d.insert("col".into(), EvalValue::Str(str_a!(0)));
            d.insert("val".into(), EvalValue::Float(num_a!(1)));
        }

        // f.ratio("col_num", "col_den") → =NUM_ROW / DEN_ROW
        // Ejemplo: f.ratio("sales", "target") → =B6/C6
        "ratio" | "div" => {
            d.insert("_f".into(),   EvalValue::Str("ratio".into()));
            d.insert("col".into(),  EvalValue::Str(str_a!(0)));
            d.insert("col2".into(), EvalValue::Str(str_a!(1)));
        }

        // f.diff("col1", "col2") → =COL1_ROW - COL2_ROW
        // Ejemplo: f.diff("sales", "target") → =B6-C6
        "diff" | "sub" => {
            d.insert("_f".into(),   EvalValue::Str("diff".into()));
            d.insert("col".into(),  EvalValue::Str(str_a!(0)));
            d.insert("col2".into(), EvalValue::Str(str_a!(1)));
        }

        // f.mul("col", factor) → =COL_ROW * factor
        // Ejemplo: f.mul("price", 1.19) → =B6*1.19
        "mul" => {
            d.insert("_f".into(),  EvalValue::Str("mul".into()));
            d.insert("col".into(), EvalValue::Str(str_a!(0)));
            d.insert("val".into(), EvalValue::Float(num_a!(1)));
        }

        // f.sum("col") → =SUM($COL$start:$COL$end)  — mismo valor en cada fila
        // Útil como denominador de porcentaje del total
        "sum" => {
            d.insert("_f".into(),  EvalValue::Str("sum".into()));
            d.insert("col".into(), EvalValue::Str(str_a!(0)));
        }

        // f.avg("col") → =AVERAGE($COL$start:$COL$end)
        "avg" | "average" => {
            d.insert("_f".into(),  EvalValue::Str("avg".into()));
            d.insert("col".into(), EvalValue::Str(str_a!(0)));
        }

        // f.cumulative("col") → =SUM($COL$start:COL_ROW)  — se expande por fila
        "cumulative" | "running" => {
            d.insert("_f".into(),  EvalValue::Str("cumulative".into()));
            d.insert("col".into(), EvalValue::Str(str_a!(0)));
        }

        // f.rank("col", "desc"|"asc") → =RANK(COL_ROW, $COL$start:$COL$end, order)
        // Ejemplo: f.rank("sales", "desc") → =RANK(B6,$B$2:$B$10,0)
        "rank" => {
            let dir = if args.len() > 1 { str_a!(1) } else { "desc".to_string() };
            d.insert("_f".into(),  EvalValue::Str("rank".into()));
            d.insert("col".into(), EvalValue::Str(str_a!(0)));
            d.insert("dir".into(), EvalValue::Str(dir));
        }

        // f.if_("col", op, val, then, else)
        // → =IF(COL_ROW op val, then, else)
        // Ejemplo: f.if_("sales", ">", 80000, "A", "B") → =IF(B6>80000,"A","B")
        // Operadores: > < >= <= == !=
        "if_" | "if" => {
            let val    = args.get(2).cloned().unwrap_or(EvalValue::Null);
            let then_v = args.get(3).cloned().unwrap_or(EvalValue::Null);
            let else_v = args.get(4).cloned().unwrap_or(EvalValue::Null);
            d.insert("_f".into(),    EvalValue::Str("if".into()));
            d.insert("col".into(),   EvalValue::Str(str_a!(0)));
            d.insert("op".into(),    EvalValue::Str(str_a!(1)));
            d.insert("val".into(),   val);
            d.insert("then".into(),  then_v);
            d.insert("else_".into(), else_v);
        }

        f => return Err(format!(
            "excel.f.{}: función no reconocida.\n\
             Disponibles: pct, ratio, diff, mul, sum, avg, cumulative, rank, if_",
            f
        )),
    }

    Ok(EvalValue::Dict(d))
}
