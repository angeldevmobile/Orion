use crate::eval_value::EvalValue;
use minijinja::Environment;

pub fn call(function: &str, args: Vec<EvalValue>) -> Result<EvalValue, String> {
    match function {
        // renderizar(template_str, vars_dict) → String
        "renderizar" | "render" => {
            if args.len() < 2 { return Err("template.renderizar requiere (template, vars)".into()); }
            let tmpl = to_str(&args[0]);
            let vars = crate::modules::json_mod::eval_to_json(args[1].clone());
            render_str(&tmpl, vars)
        }
        // desde_archivo(path, vars_dict) → String
        "desde_archivo" | "from_file" => {
            if args.len() < 2 { return Err("template.desde_archivo requiere (path, vars)".into()); }
            let path = to_str(&args[0]);
            let vars = crate::modules::json_mod::eval_to_json(args[1].clone());
            let tmpl = std::fs::read_to_string(&path)
                .map_err(|e| format!("template.desde_archivo '{}': {}", path, e))?;
            render_str(&tmpl, vars)
        }
        f => Err(format!("template.{}() no existe", f)),
    }
}

fn render_str(template: &str, ctx: serde_json::Value) -> Result<EvalValue, String> {
    let mut env = Environment::new();
    env.add_template("t", template).map_err(|e| format!("template: {}", e))?;
    let result = env.get_template("t")
        .map_err(|e| format!("template: {}", e))?
        .render(ctx)
        .map_err(|e| format!("template.renderizar: {}", e))?;
    Ok(EvalValue::Str(result))
}

fn to_str(v: &EvalValue) -> String {
    match v { EvalValue::Str(s) => s.clone(), other => format!("{}", other) }
}
