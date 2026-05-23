fn main() {
    // Compila orion.rc (que embebe orion.manifest con activeCodePage=UTF-8)
    // en el ejecutable como recurso RT_MANIFEST.
    // Funciona con MSVC y GNU toolchains en Windows 10 1903+ / Windows 11.
    #[cfg(target_os = "windows")]
    embed_resource::compile("orion.rc", embed_resource::NONE);
}
