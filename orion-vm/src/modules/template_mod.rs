use crate::eval_value::EvalValue;
use minijinja::{AutoEscape, Environment, UndefinedBehavior};
use std::path::{Path, PathBuf};

pub fn call(function: &str, args: Vec<EvalValue>) -> Result<EvalValue, String> {
    match function {
        // render(template_str, vars_dict, opts?) → String  — escapes HTML unless { escape: "none" }
        "render" | "renderizar" => {
            if args.len() < 2 { return Err("template.render requires (template, vars)".into()); }
            let opts = Opts::read(args.get(2))?;
            let base = opts.dir.clone().unwrap_or_else(|| PathBuf::from("."));
            render(MAIN, to_str(&args[0]), &args[1], base, opts)
        }
        // from_file(path, vars_dict, opts?) → String  — escapes by file extension; extends/include from its folder
        "from_file" | "desde_archivo" => {
            if args.len() < 2 { return Err("template.from_file requires (path, vars)".into()); }
            let opts = Opts::read(args.get(2))?;
            let path = PathBuf::from(to_str(&args[0]));
            let source = std::fs::read_to_string(&path)
                .map_err(|e| format!("template.from_file '{}': {}", path.display(), e))?;
            let name = path.file_name().map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_else(|| MAIN.to_string());
            let base = opts.dir.clone().unwrap_or_else(|| {
                path.parent().map(Path::to_path_buf).unwrap_or_else(|| PathBuf::from("."))
            });
            render(&name, source, &args[1], base, opts)
        }
        // escape(text) → String  — escapes & < > " ' / like template autoescape
        "escape" => {
            let text = args.first().map(to_str).unwrap_or_default();
            Ok(EvalValue::Str(escape_html(&text)))
        }
        f => Err(format!("template.{}() does not exist", f)),
    }
}

const MAIN: &str = "<template>";

struct Opts {
    escape: Option<AutoEscape>,
    dir:    Option<PathBuf>,
    strict: bool,
}

impl Opts {
    fn read(v: Option<&EvalValue>) -> Result<Opts, String> {
        let mut o = Opts { escape: None, dir: None, strict: false };
        let Some(EvalValue::Dict(d)) = v else { return Ok(o) };
        if let Some(e) = d.get("escape") {
            o.escape = Some(match e {
                EvalValue::Bool(true) => AutoEscape::Html,
                EvalValue::Bool(false) => AutoEscape::None,
                EvalValue::Str(s) => match s.as_str() {
                    "html" => AutoEscape::Html,
                    "json" => AutoEscape::Json,
                    "none" => AutoEscape::None,
                    other => return Err(format!(
                        "template: escape '{}' is not valid; use \"html\", \"json\" or \"none\"", other)),
                },
                _ => return Err("template: escape must be \"html\", \"json\", \"none\" or a bool".into()),
            });
        }
        if let Some(d) = d.get("dir") { o.dir = Some(PathBuf::from(to_str(d))); }
        if let Some(EvalValue::Bool(b)) = d.get("strict") { o.strict = *b; }
        Ok(o)
    }
}

/// Escaping by file extension; unknown or no extension means HTML (the safe choice).
fn escape_for(name: &str) -> AutoEscape {
    let ext = name.rsplit_once('.').map(|(_, e)| e.to_ascii_lowercase()).unwrap_or_default();
    match ext.as_str() {
        "json" | "js" => AutoEscape::Json,
        "txt" | "text" | "md" | "csv" | "eml" | "sql" => AutoEscape::None,
        _ => AutoEscape::Html,
    }
}

/// A template name may only point inside the templates folder: no absolute
/// paths (they replace `base` in `join`) and no `..`.
fn confined(name: &str) -> Result<&Path, minijinja::Error> {
    use std::path::Component;
    let p = Path::new(name);
    if p.components().all(|c| matches!(c, Component::Normal(_) | Component::CurDir)) {
        Ok(p)
    } else {
        Err(minijinja::Error::new(minijinja::ErrorKind::InvalidOperation,
            format!("template '{}' is outside the templates folder", name)))
    }
}

fn render(name: &str, source: String, vars: &EvalValue, base: PathBuf, opts: Opts)
    -> Result<EvalValue, String>
{
    let mut env = Environment::new();
    let main = name.to_string();
    let forced = opts.escape.clone();
    env.set_auto_escape_callback(move |n| match &forced {
        Some(e) if n == main => e.clone(),
        _ => escape_for(n),
    });
    if opts.strict { env.set_undefined_behavior(UndefinedBehavior::Strict); }
    // extends / include / import resolve relative to `base`.
    env.set_loader(move |n| match std::fs::read_to_string(base.join(confined(n)?)) {
        Ok(s) => Ok(Some(s)),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(minijinja::Error::new(minijinja::ErrorKind::InvalidOperation,
            format!("could not read template '{}': {}", n, e))),
    });
    env.add_template_owned(name.to_string(), source).map_err(|e| format!("template: {}", e))?;
    let ctx = crate::modules::json_mod::eval_to_json(vars.clone());
    let out = env.get_template(name)
        .and_then(|t| t.render(ctx))
        .map_err(|e| format!("template: {}", e))?;
    Ok(EvalValue::Str(out))
}

pub(crate) fn escape_html(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + s.len() / 8);
    for c in s.chars() {
        match c {
            '&'  => out.push_str("&amp;"),
            '<'  => out.push_str("&lt;"),
            '>'  => out.push_str("&gt;"),
            '"'  => out.push_str("&quot;"),
            '\'' => out.push_str("&#x27;"),
            '/'  => out.push_str("&#x2f;"),
            _    => out.push(c),
        }
    }
    out
}

fn to_str(v: &EvalValue) -> String {
    match v { EvalValue::Str(s) => s.clone(), other => format!("{}", other) }
}

#[cfg(test)]
mod tests {
    use super::*;
    use indexmap::IndexMap;

    fn vars(k: &str, v: &str) -> EvalValue {
        let mut d = IndexMap::new();
        d.insert(k.to_string(), EvalValue::Str(v.to_string()));
        EvalValue::Dict(d)
    }

    fn opts(k: &str, v: EvalValue) -> EvalValue {
        let mut d = IndexMap::new();
        d.insert(k.to_string(), v);
        EvalValue::Dict(d)
    }

    fn out(r: Result<EvalValue, String>) -> String {
        match r { Ok(EvalValue::Str(s)) => s, other => panic!("{:?}", other.err()) }
    }

    #[test]
    fn render_escapes_html_by_default() {
        let r = call("render", vec![EvalValue::Str("<p>{{ n }}</p>".into()), vars("n", "<script>x</script>")]);
        assert_eq!(out(r), "<p>&lt;script&gt;x&lt;&#x2f;script&gt;</p>");
    }

    #[test]
    fn render_safe_filter_and_escape_none() {
        let r = call("render", vec![EvalValue::Str("{{ n|safe }}".into()), vars("n", "<b>ok</b>")]);
        assert_eq!(out(r), "<b>ok</b>");
        let r = call("render", vec![EvalValue::Str("{{ n }}".into()), vars("n", "<b>"),
                                    opts("escape", EvalValue::Str("none".into()))]);
        assert_eq!(out(r), "<b>");
    }

    #[test]
    fn render_strict_fails_on_undefined_variable() {
        let r = call("render", vec![EvalValue::Str("{{ missing }}".into()), vars("n", "x"),
                                    opts("strict", EvalValue::Bool(true))]);
        assert!(r.is_err());
    }

    #[test]
    fn from_file_extends_include_and_escape_by_extension() {
        let dir = std::env::temp_dir().join(format!("orion_tpl_{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("base.html"), "<main>{% block c %}{% endblock %}</main>").unwrap();
        std::fs::write(dir.join("footer.html"), "<footer>{{ n }}</footer>").unwrap();
        std::fs::write(dir.join("child.html"),
            "{% extends \"base.html\" %}{% block c %}{{ n }}{% include \"footer.html\" %}{% endblock %}").unwrap();
        std::fs::write(dir.join("note.txt"), "Hello {{ n }}").unwrap();

        let path = dir.join("child.html").to_string_lossy().into_owned();
        let r = call("from_file", vec![EvalValue::Str(path), vars("n", "<i>")]);
        assert_eq!(out(r), "<main>&lt;i&gt;<footer>&lt;i&gt;</footer></main>");

        let path = dir.join("note.txt").to_string_lossy().into_owned();
        let r = call("from_file", vec![EvalValue::Str(path), vars("n", "<i>")]);
        assert_eq!(out(r), "Hello <i>");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn escape_text() {
        let r = call("escape", vec![EvalValue::Str("a<b>&\"c'/".into())]);
        assert_eq!(out(r), "a&lt;b&gt;&amp;&quot;c&#x27;&#x2f;");
    }

    #[test]
    fn invalid_escape_gives_clear_error() {
        let r = call("render", vec![EvalValue::Str("x".into()), vars("n", "x"),
                                    opts("escape", EvalValue::Str("xml".into()))]);
        assert!(r.unwrap_err().contains("\"html\", \"json\" or \"none\""));
    }
}
