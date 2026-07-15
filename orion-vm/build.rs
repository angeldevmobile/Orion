fn main() {
    // Compila orion.rc (que embebe orion.manifest con activeCodePage=UTF-8)
    // en el ejecutable como recurso RT_MANIFEST.
    // Funciona con MSVC y GNU toolchains en Windows 10 1903+ / Windows 11.
    #[cfg(target_os = "windows")]
    embed_resource::compile("orion.rc", embed_resource::NONE);

    regen_builtins();
}

/// Regenera la typeshed (src/cli/builtins_gen.rs) en cada build para que el
/// hover/autocompletado del LSP nunca quede desfasado de los módulos.
/// Si `node` no está disponible se usa el archivo generado ya commiteado:
/// el build nunca falla por esto, solo avisa.
fn regen_builtins() {
    // Solo re-correr cuando cambian los módulos o el propio generador.
    println!("cargo:rerun-if-changed=src/modules");
    println!("cargo:rerun-if-changed=scripts/gen_builtins.js");

    let out = std::process::Command::new("node")
        .arg("scripts/gen_builtins.js")
        .output();
    match out {
        Ok(o) if o.status.success() => {}
        Ok(o) => println!(
            "cargo:warning=gen_builtins.js falló ({}); se usa la typeshed commiteada",
            String::from_utf8_lossy(&o.stderr).lines().next().unwrap_or("?")
        ),
        Err(_) => println!(
            "cargo:warning=node no disponible; typeshed no regenerada (se usa la commiteada)"
        ),
    }
}
