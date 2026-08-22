//! Alias en español obsoletos — SPEC.md sección 11.
//!
//! El inglés es el nombre canónico de la stdlib; los nombres españoles que
//! vinieron primero siguen funcionando durante toda la 0.1.x y está previsto
//! retirarlos. Esta tabla existe para avisar antes de ese día, no el día.
//!
//! **Por qué es una lista curada y no se deriva del registro.** El registro
//! marca TODOS los alias con "Alias de modulo.principal.", pero no todos son
//! traducciones: `log.warn`, `log.err` y `log.debug` comparten brazo con
//! `log.info` y son niveles distintos; `state.increment` es alias inglés de
//! `state.incr`; `router.post` de `router.get`. Avisar de esos sería decirle
//! al usuario que su código está obsoleto cuando no lo está. Distinguir español
//! de inglés no es derivable, así que se decide una vez y se escribe aquí.
//!
//! Al añadir un alias español nuevo, añádelo también aquí. `deprecated_sync`
//! en tests/ comprueba que cada entrada siga existiendo en el registro.

/// ("modulo.alias_español", "nombre_inglés"). ORDENADA: se busca por bisección.
static ALIAS: &[(&str, &str)] = &[
    ("auth.verificar", "verify"),
    ("auth.verificar_token", "decode_token"),
    ("cache.claves", "keys"),
    ("cache.eliminar", "del"),
    ("cache.existe", "has"),
    ("cache.guardar", "set"),
    ("cache.limpiar", "clear"),
    ("cache.obtener", "get"),
    ("cache.tamaño", "size"),
    ("chan.cerrada", "is_closed"),
    ("chan.cerrar", "close"),
    ("chan.crear", "create"),
    ("chan.eliminar", "delete"),
    ("chan.enviar", "send"),
    ("chan.intentar_recibir", "try_recv"),
    ("chan.lista", "list"),
    ("chan.recibir", "recv"),
    ("chan.seleccionar", "select"),
    ("chan.tamaño", "len"),
    ("chan.try_recibir", "try_recv"),
    ("db.cerrar", "close"),
    ("db.consulta", "query"),
    ("db.copiar", "copy"),
    ("db.copiar_archivo", "copy_file"),
    ("db.ejecutar", "exec"),
    ("db.insertar", "insert"),
    ("db.tablas", "tables"),
    ("db.transaccion", "transaction"),
    ("db.uno", "first"),
    ("excel.agrupar", "group"),
    ("excel.columna", "column"),
    ("excel.cruzar", "join"),
    ("excel.deduplicar", "dedupe"),
    ("excel.estadisticas", "stats"),
    ("excel.filtrar", "filter"),
    ("excel.ordenar", "sort"),
    ("excel.promedio", "avg_col"),
    ("excel.rellenar", "fill_null"),
    ("excel.renombrar_col", "rename_col"),
    ("excel.seleccionar", "select_cols"),
    ("excel.sumar", "sum_col"),
    ("excel.unir", "concat"),
    ("format.centrar", "center"),
    ("format.duracion", "duration"),
    ("format.moneda", "currency"),
    ("format.numero", "number"),
    ("format.porcentaje", "percent"),
    ("format.separador", "divider"),
    ("format.tabla", "table"),
    ("format.truncar", "truncate"),
    ("fs.guardar_b64", "write_b64"),
    ("fs.leer_b64", "read_b64"),
    ("graph.arista", "edge"),
    ("graph.camino", "path"),
    ("graph.crear", "create"),
    ("graph.eliminar", "delete"),
    ("graph.nodo", "node"),
    ("graph.nodos", "nodes"),
    ("graph.vecinos", "neighbors"),
    ("gui.abrir_archivo", "file_open"),
    ("gui.abrir_carpeta", "folder_open"),
    ("gui.campos", "fields"),
    ("gui.encabezado", "header"),
    ("gui.guardar_archivo", "file_save"),
    ("gui.metricas", "stats"),
    ("gui.opciones", "chips"),
    ("gui.seccion", "section"),
    ("insight.umbral", "threshold"),
    ("mail.enviar", "send"),
    ("mail.enviar_html", "send_html"),
    ("pdf.crear", "create"),
    ("pdf.desde_imagen", "from_image"),
    ("pdf.extraer_texto", "read"),
    ("pdf.leer", "read"),
    ("pdf.marca", "watermark"),
    ("pdf.paginar", "paginate"),
    ("pdf.paginas", "pages"),
    ("pdf.plantilla", "template"),
    ("pdf.reporte", "report"),
    ("pdf.texto", "text"),
    ("process.argumento", "arg"),
    ("process.argumentos", "args"),
    ("quantum.circuito", "circuit"),
    ("queue.crear", "create"),
    ("queue.eliminar", "delete"),
    ("queue.enviar", "push"),
    ("queue.espiar", "peek"),
    ("queue.lista", "list"),
    ("queue.recibir", "pop"),
    ("queue.tamaño", "size"),
    ("queue.vaciar", "clear"),
    ("session.cuenta", "count"),
    ("session.destruir", "destroy"),
    ("session.eliminar", "delete"),
    ("session.existe", "has"),
    ("session.guardar", "set"),
    ("session.obtener", "get"),
    ("session.podar", "sweep"),
    ("session.todo", "all"),
    ("state.claves", "keys"),
    ("state.eliminar", "delete"),
    ("state.existe", "has"),
    ("state.guardar", "set"),
    ("state.limpiar", "clear"),
    ("state.obtener", "get"),
    ("state.persistir", "persist"),
    ("state.tamaño", "len"),
    ("state.todo", "all"),
    ("task.ahora", "now"),
    ("task.dormir", "sleep"),
    ("task.iniciar", "start"),
    ("task.medir", "elapsed"),
    ("task.reiniciar", "reset"),
    ("task.repetir", "repeat"),
    ("template.desde_archivo", "from_file"),
    ("template.renderizar", "render"),
    ("validate.alfa", "alpha"),
    ("validate.alfanumerico", "alphanumeric"),
    ("validate.longitud", "length"),
    ("validate.numero", "number"),
    ("validate.requerido", "required"),
    ("validate.todo", "all"),
    ("vector.buscar", "search"),
    ("vector.cargar", "load"),
    ("vector.eliminar", "remove"),
    ("vector.guardar", "save"),
    ("vector.limpiar", "clear"),
    ("vector.tamaño", "size"),
    ("vision.bordes", "edges"),
    ("vision.contraste", "contrast"),
    ("vision.invertir", "invert"),
    ("vision.leer_texto", "ocr"),
    ("vision.nitidez", "sharpen"),
    ("vision.umbral", "threshold"),
    ("watch.dejar", "unwatch"),
    ("watch.estado", "stat"),
    ("watch.lista", "list"),
    ("watch.modificado", "changed"),
    ("watch.observar", "watch"),
    ("ws.cerrar", "close"),
    ("ws.conectar", "connect"),
    ("ws.conexiones", "connections"),
    ("ws.enviar", "send"),
    ("ws.recibir", "recv"),
];

/// El nombre inglés de un alias español, si `function` lo es.
///
/// `module` es el nombre CANÓNICO del módulo (el typechecker ya resolvió
/// `formato` a `format` antes de llegar aquí).
pub fn canonical_for(module: &str, function: &str) -> Option<&'static str> {
    let clave = format!("{module}.{function}");
    ALIAS.binary_search_by(|(k, _)| (*k).cmp(clave.as_str()))
        .ok()
        .map(|i| ALIAS[i].1)
}

/// Módulos cuyo propio nombre es un alias español obsoleto.
///
/// `df` y `embeddings` NO están: son abreviaturas inglesas deliberadas, no
/// herencia del español, y seguirán existiendo.
pub fn module_canonical(name: &str) -> Option<&'static str> {
    match name {
        "tarea"   => Some("task"),
        "cola"    => Some("queue"),
        "formato" => Some("format"),
        "grafo"   => Some("graph"),
        _ => None,
    }
}

/// Todas las entradas, para el test de sincronía con el registro.
pub fn todos() -> &'static [(&'static str, &'static str)] { ALIAS }
