import importlib.util
import inspect
import sys
import os
import types

sys.path.append(os.path.join(os.path.dirname(__file__), ".."))
from core.control import eval_match
from core.functions import register_function, get_function, register_native_function
from core.types import OrionDict, OrionList, OrionString, OrionNumber, OrionBool, OrionDate, null_safe
from core.errors import (
    OrionRuntimeError,
    OrionTypeError,
    OrionNameError,
    OrionFunctionError,
)

from modules import json as orion_json
from lib import collections
# === INTEGRACIÓN DE ORION CODE & SHOW ENGINE ===
from modules import code, show 
from lib.io import io_show
from lib import math as orion_math
from modules import strings


class ContinueException(Exception):
    """Excepción para manejar continue en loops"""
    pass

class BreakException(Exception):
    """Excepción para manejar break en loops"""
    pass
# === INTEGRACIÓN CON HOJAS DE CÁLCULO ORION ===
try:
    from modules.spreadsheet import OrionSpreadsheet, create, attach, register as spreadsheet_register
    from modules.sheets_localbridge import LocalSheetBridge, OSync
    from modules.linksheet import LinkSheet
    from core.protocol_osync import OSyncProtocol
    SPREADSHEET_ENABLED = True
    SPREADSHEET_FUNCTIONS = {
        # OrionSpreadsheet functions
        "create_sheet": create,
        "attach_sheet": LocalSheetBridge.attach,  
        "register_sheet": spreadsheet_register,
        
        # LocalSheetBridge functions
        "sheet_register": LocalSheetBridge.register,
        "sheet_attach": LocalSheetBridge.attach,
        
        # OSync functions
        "sync_push": OSync.push,
        "sync_pull": OSync.pull,
        "sync_status": OSync.status,
        "sync_list": OSync.list_synced,
        
        # LinkSheet functions
        "link_sheet": LinkSheet.link,
        "push_remote": LinkSheet.push,
        "pull_remote": LinkSheet.pull,
        
        # OSyncProtocol function
        "osync_execute": OSyncProtocol.execute
    }
    code.info("Módulo de hojas de cálculo Orion cargado exitosamente", module="spreadsheet-core")
except ImportError as e:
    SPREADSHEET_ENABLED = False
    SPREADSHEET_FUNCTIONS = {}
    code.warn(f"Módulo de hojas de cálculo no disponible: {e}", module="spreadsheet-core")

# === INTEGRACIÓN DEL MÓDULO AI ===
try:
    from stdlib.ai import orion_export, think, quantum_embed, recall
    AI_ENABLED = True
    AI_FUNCTIONS = orion_export()
    code.info("Módulo AI Orion cargado exitosamente", module="ai-core")
except ImportError as e:
    AI_ENABLED = False
    AI_FUNCTIONS = {}
    code.warn(f"Módulo AI no disponible: {e}", module="ai-core")
    
# ==========================================
try: 
    from stdlib.cosmos import orion_export, cosmos, Body, Universe
    COSMOS_ENABLED = True
    COSMOS_FUNCTIONS = orion_export()
    code.info("Módulo Cosmos Orion cargado exitosamente", module="cosmos-core")
except ImportError as e:
    COSMOS_ENABLED = False
    COSMOS_FUNCTIONS = {}
    code.warn(f"Módulo Cosmos no disponible: {e}", module="cosmos-core")
    
# =========================================
try:
    from stdlib.crypto import orion_export as crypto_export, crypto, hash, encrypt, decrypt, sign, verify
    CRYPTO_ENABLED = True
    CRYPTO_FUNCTIONS = crypto_export()
    code.info("Módulo Crypto Orion cargado exitosamente", module="crypto-core")
except ImportError as e:
    CRYPTO_ENABLED = False
    CRYPTO_FUNCTIONS = {}
    code.warn(f"Módulo Crypto no disponible: {e}", module="crypto-core")
    
# ============================================================
try:
    from stdlib.insight import orion_export as insight_export, extract_text_blocks, extract_tables, extract_metadata, extract_signatures, summarize
    INSIGHT_ENABLED = True
    INSIGHT_FUNCTIONS = insight_export()
    code.info("Módulo Insight Orion cargado exitosamente", module="insight-core")
except ImportError as e:
    INSIGHT_ENABLED = False
    INSIGHT_FUNCTIONS = {}
    code.warn(f"Módulo Insight no disponible: {e}", module="insight-core")
    
# ============================================================
try:
    from stdlib.matrix import orion_export as matrix_export, matrix, SmartMatrix, add, mul, transpose, det, inverse, neuralify, morph
    MATRIX_ENABLED = True
    MATRIX_FUNCTIONS = matrix_export()
    code.info("Módulo Matrix Orion cargado exitosamente", module="matrix-core")
except ImportError as e:
    MATRIX_ENABLED = False
    MATRIX_FUNCTIONS = {}
    code.warn(f"Módulo Matrix no disponible: {e}", module="matrix-core")   
    
# ============================================================
try:
    from stdlib.quantum import orion_export as quantum_export, quantum, qubit, bell_pair, measure, apply_gate, tensor, fidelity
    QUANTUM_ENABLED = True
    QUANTUM_FUNCTIONS = quantum_export()
    code.info("Módulo Quantum Orion cargado exitosamente", module="quantum-core")
except ImportError as e:
    QUANTUM_ENABLED = False
    QUANTUM_FUNCTIONS = {}
    code.warn(f"Módulo Quantum no disponible: {e}", module="quantum-core")

try:
    from stdlib.timewarp import orion_export as timewarp_export, timewarp, WarpClock, TimeLine, future, warp_speed, wait, measureMtime
    TIMEWARP_ENABLED = True
    TIMEWARP_FUNCTIONS = timewarp_export()
    code.info("Módulo TimeWarp Orion cargado exitosamente", module="timewarp-core")
except ImportError as e:
    TIMEWARP_ENABLED = False
    TIMEWARP_FUNCTIONS = {}
    code.warn(f"Módulo TimeWarp no disponible: {e}", module="timewarp-core")

try:
    from stdlib.vision import orion_export as vision_export, load, save, resize, smart_crop, dhash, detect_faces, blur_faces, ImagePipeline
    VISION_ENABLED = True
    VISION_FUNCTIONS = vision_export()
    code.info("Módulo Vision Orion cargado exitosamente", module="vision-core")
except ImportError as e:
    VISION_ENABLED = False
    VISION_FUNCTIONS = {}
    code.warn(f"Módulo Vision no disponible: {e}", module="vision-core")

# === ORION VISUAL ENGINE FUNCTIONS ===
NATIVE_FUNCTIONS = {
    # Orion CODE Engine
    "trace_start": code.trace_start,
    "trace_end": code.trace_end,
    "progress": code.progress,
    "divider": code.divider,
    "frame": code.frame,
    "pulse": code.pulse,
    
    # Orion SHOW Engine
    "show": show.show,
    
    # Orion Log Levels
    "info": code.info,
    "ok": code.ok,
    "warn": code.warn,
    "error": code.error,
    "debug": code.debug,
    "trace": code.trace,
}

# Agregar funciones AI a las funciones nativas si están disponibles
if AI_ENABLED:
    NATIVE_FUNCTIONS.update({
        # Funciones principales AI
        "think": think,
        "embed": quantum_embed,
        "recall": recall,
        
        # Aliases cortos del módulo AI
        "fit": AI_FUNCTIONS.get("fit"),
        "predict": AI_FUNCTIONS.get("predict"),
        "sim": AI_FUNCTIONS.get("sim"),
        "dist": AI_FUNCTIONS.get("dist"),
        "cluster": AI_FUNCTIONS.get("cluster"),
        "normalize": AI_FUNCTIONS.get("normalize"),
        "accuracy": AI_FUNCTIONS.get("accuracy"),
        "mse": AI_FUNCTIONS.get("mse"),
    })

if COSMOS_ENABLED:
    NATIVE_FUNCTIONS.update({
        "cosmos": cosmos,
        "gravity": COSMOS_FUNCTIONS.get("gravity"),
        "energy": COSMOS_FUNCTIONS.get("energy"),
        "dust": COSMOS_FUNCTIONS.get("dust"),
        "Body": Body,
        "Universe": Universe
    })
    
if CRYPTO_ENABLED:
    NATIVE_FUNCTIONS.update({
        "crypto": crypto,
        "hash": hash,
        "encrypt": encrypt,
        "decrypt": decrypt,
        "sign": sign,
        "verify": verify,
        "uuid": CRYPTO_FUNCTIONS.get("uuid"),
        "token": CRYPTO_FUNCTIONS.get("token"),
        "entropy": CRYPTO_FUNCTIONS.get("entropy"),
        "context_token": CRYPTO_FUNCTIONS.get("context_token"),
    })
    
if INSIGHT_ENABLED:
    NATIVE_FUNCTIONS.update({
        "extract_text_blocks": extract_text_blocks,
        "extract_tables": extract_tables,
        "extract_metadata": extract_metadata,
        "extract_signatures": extract_signatures,
        "summarize": summarize
    })
    
if MATRIX_ENABLED:
    NATIVE_FUNCTIONS.update({
        "matrix": matrix,
        "SmartMatrix": SmartMatrix,
        "add": add,
        "mul": mul,
        "transpose": transpose,
        "det": det,
        "inverse": inverse,
        "trace": MATRIX_FUNCTIONS.get("trace"),
        "rot2D": MATRIX_FUNCTIONS.get("rot2D"),
        "rot3D": MATRIX_FUNCTIONS.get("rot3D"),
        "neuralify": neuralify,
        "amplify": MATRIX_FUNCTIONS.get("amplify"),
        "collapse": MATRIX_FUNCTIONS.get("collapse"),
        "morph": morph,
        "neuralify": neuralify
    })
    
if QUANTUM_ENABLED:
    NATIVE_FUNCTIONS.update({
        "quantum": quantum,
        "qubit": qubit,
        "bell_pair": bell_pair,
        "measure": measure,
        "apply_gate": apply_gate,
        "tensor": tensor,
        "fidelity": fidelity,
        "zero": QUANTUM_FUNCTIONS.get("zero"),
        "one": QUANTUM_FUNCTIONS.get("one"),
        "rand": QUANTUM_FUNCTIONS.get("rand"),
        "bloch": QUANTUM_FUNCTIONS.get("bloch"),
        "apply_circuit": QUANTUM_FUNCTIONS.get("apply_circuit"),
        "state_from_bits": QUANTUM_FUNCTIONS.get("state_from_bits"),
        "expand_gate": QUANTUM_FUNCTIONS.get("expand_gate"),
        "control_gate": QUANTUM_FUNCTIONS.get("control_gate")
    })
    
if TIMEWARP_ENABLED:
    NATIVE_FUNCTIONS.update({
        "timewarp": timewarp,
        "WarpClock": WarpClock,
        "TimeLine": TimeLine,
        "future": future,
        "warp_speed": warp_speed,
        "wait": wait,
        "measureMtime": measureMtime,
        "block_future": TIMEWARP_FUNCTIONS.get("block_future"),
        "block_past": TIMEWARP_FUNCTIONS.get("block_past")
    })
    
if VISION_ENABLED:
    NATIVE_FUNCTIONS.update({
        "load_image": load,
        "save_image": save,
        "resize_image": resize,
        "smart_crop": smart_crop,
        "dhash": dhash,
        "detect_faces": detect_faces,
        "blur_faces": blur_faces,
        "ImagePipeline": ImagePipeline,
        "thumbnail": VISION_FUNCTIONS.get("thumbnail"),
        "crop": VISION_FUNCTIONS.get("crop"),
        "hamming": VISION_FUNCTIONS.get("hamming"),
        "dominant_colors": VISION_FUNCTIONS.get("dominant_colors"),
        "hist_eq": VISION_FUNCTIONS.get("hist_eq"),
        "auto_enhance": VISION_FUNCTIONS.get("auto_enhance"),
        "scan_text": VISION_FUNCTIONS.get("scan_text"),
        "seam_carve": VISION_FUNCTIONS.get("seam_carve")
    })

def lookup_variable(name, variables):
    """Busca una variable en el scope actual."""
    if name in variables:
        return variables[name]
    # Usa el error personalizado de Orion
    raise OrionNameError(name)

def substring(s, start, end=None):
    """Devuelve el substring de s desde start hasta end (como en Python)."""
    if end is not None:
        return str(s)[int(start):int(end)]
    return str(s)[int(start):]

def _register_builtin_functions(functions):
    import types
    from lib import io 

    builtin_modules = [collections, io, orion_math, strings]
    for mod in builtin_modules:
        for k in dir(mod):
            if not k.startswith("_"):
                v = getattr(mod, k)
                if isinstance(v, types.FunctionType):
                    register_native_function(functions, k, v)


    # === Conversión flexible list() ===
    def orion_list(value):
        """Convierte cualquier objeto iterable a OrionList."""
        if isinstance(value, (OrionList, list)):
            return OrionList(value)
        elif isinstance(value, (OrionDict, dict)):
            return OrionList(list(value.keys()))
        elif isinstance(value, (OrionString, str)):
            return OrionList(list(value))
        elif hasattr(value, "__iter__"):
            return OrionList(list(value))
        else:
            raise OrionTypeError("Tipo no convertible a lista")
        
    # Función type() mejorada
    def handle_builtin_type(obj):
        """Función type() nativa de Orion que devuelve string del tipo"""
        if isinstance(obj, OrionList) or isinstance(obj, list):
            return "list"
        elif isinstance(obj, OrionString) or isinstance(obj, str):
            return "string"
        elif isinstance(obj, OrionNumber) or isinstance(obj, (int, float)):
            return "number"
        elif isinstance(obj, OrionBool) or isinstance(obj, bool):
            return "bool"
        elif isinstance(obj, OrionDict) or isinstance(obj, dict):
            return "dict"
        else:
            return type(obj).__name__.lower()

    # Registrar funciones nativas de Python necesarias
    register_native_function(functions, "len", len)
    register_native_function(functions, "range", range)
    register_native_function(functions, "str", str)
    register_native_function(functions, "int", int)
    register_native_function(functions, "float", float)
    register_native_function(functions, "type", handle_builtin_type)  # Función type() mejorada
    register_native_function(functions, "auto", lambda *args, **kwargs: args[0] if args else None)
    register_native_function(functions, "substring", substring)
    register_native_function(functions, "to_native", to_native)
    register_native_function(functions, "list", orion_list)
    
    # Asegurar que type() esté disponible también como función standalone
    functions["type"] = {
        "type": "NATIVE_FN", 
        "impl": handle_builtin_type
    }
    # === REGISTRAR FUNCIONES SPREADSHEET COMO BUILT-INS ===
    if SPREADSHEET_ENABLED:
        for sheet_func_name, sheet_func in SPREADSHEET_FUNCTIONS.items():
            if callable(sheet_func):
                register_native_function(functions, sheet_func_name, sheet_func)
        
        # Registrar aliases cortos y funciones específicas
        register_native_function(functions, "sheet", lambda filename: create(filename))
        register_native_function(functions, "sync", lambda sheet_id, mode="push": 
            OSync.push(sheet_id) if mode == "push" else OSync.pull(sheet_id))
        register_native_function(functions, "osync_cmd", OSyncProtocol.execute)
        
        # Funciones específicas para manejo de hojas
        register_native_function(functions, "sheet_write", lambda sheet_id, cell, value: 
            LocalSheetBridge.attach(sheet_id).write(cell, value))
        register_native_function(functions, "sheet_read", lambda sheet_id, cell: 
            LocalSheetBridge.attach(sheet_id).read(cell))
        register_native_function(functions, "sheet_save", lambda sheet_id: 
            LocalSheetBridge.attach(sheet_id).save())
        
        code.ok(f"{len(SPREADSHEET_FUNCTIONS)} funciones Spreadsheet registradas como built-ins", module="spreadsheet-registry")

    # === REGISTRAR FUNCIONES AI COMO BUILT-INS ===
    if AI_ENABLED:
        # Registrar todas las funciones AI exportadas
        for ai_func_name, ai_func in AI_FUNCTIONS.items():
            if callable(ai_func):
                register_native_function(functions, ai_func_name, ai_func)
        
        # Registrar funciones AI con prefijo para evitar colisiones
        register_native_function(functions, "ai_think", think)
        register_native_function(functions, "ai_embed", quantum_embed)
        register_native_function(functions, "ai_recall", recall)
        
        code.ok(f"{len(AI_FUNCTIONS)} funciones AI registradas como built-ins", module="ai-registry")
        
    if COSMOS_ENABLED:
        for cosmos_func_name, cosmos_fun in COSMOS_FUNCTIONS.items():
            if callable(cosmos_fun):
                register_native_function(functions, cosmos_func_name, cosmos_fun)
        register_native_function(functions, "cosmos_create", lambda *args, **kwargs: cosmos("create", *args, **kwargs))
        register_native_function(functions, "cosmos_run", lambda *args, **kwargs: cosmos("run", *args, **kwargs))
        register_native_function(functions, "cosmos_dust", lambda *args, **kwargs: cosmos("dust", *args, **kwargs))
        code.ok(f"{len(COSMOS_FUNCTIONS)} funciones Cosmos registradas como built-ins", module="cosmos-registry")

    if CRYPTO_ENABLED:
        for crypto_func_name, crypto_func in CRYPTO_FUNCTIONS.items():
            if callable(crypto_func) and crypto_func_name != "__meta__":
                register_native_function(functions, crypto_func_name, crypto_func)
        register_native_function(functions, "crypto_hash", lambda *args, **kwargs: crypto("hash", *args, **kwargs))
        register_native_function(functions, "crypto_encrypt", lambda *args, **kwargs: crypto("encrypt", *args, **kwargs))
        register_native_function(functions, "crypto_decrypt", lambda *args, **kwargs: crypto("decrypt", *args, **kwargs))
        code.ok(f"{len([f for f in CRYPTO_FUNCTIONS if callable(CRYPTO_FUNCTIONS[f])])} funciones Crypto registradas como built-ins", module="crypto-registry")
    
    if INSIGHT_ENABLED:
        for insight_func_name, insight_func in INSIGHT_FUNCTIONS.items():
            if callable(insight_func) and insight_func_name != "insight":
                register_native_function(functions, insight_func_name, insight_func)
                
        # Registrar función principal insight
        if "insight" in INSIGHT_FUNCTIONS:
            insight_main = INSIGHT_FUNCTIONS["insight"]
            for func_name, func in insight_main.items():
                if callable(func):
                    register_native_function(functions, f"insight_{func_name}", func)
        
        code.ok(f"{len([f for f in INSIGHT_FUNCTIONS if callable(INSIGHT_FUNCTIONS.get(f, {}).get if isinstance(INSIGHT_FUNCTIONS.get(f), dict) else INSIGHT_FUNCTIONS.get(f))])} funciones Insight registradas como built-ins", module="insight-registry")

    if MATRIX_ENABLED:
        for matrix_func_name, matrix_func in MATRIX_FUNCTIONS.items():
            if callable(matrix_func):
                register_native_function(functions, matrix_func_name, matrix_func)
        register_native_function(functions, "matrix_add", lambda *args: matrix("add", *args))
        register_native_function(functions, "matrix_mul", lambda *args: matrix("mul", *args))
        register_native_function(functions, "matrix_det", lambda *args: matrix("det", *args))
        register_native_function(functions, "matrix_inv", lambda *args: matrix("inverse", *args))
        code.ok(f"{len(MATRIX_FUNCTIONS)} funciones Matrix registradas como built-ins", module="matrix-registry")

    if QUANTUM_ENABLED:
        for quantum_func_name, quantum_func in QUANTUM_FUNCTIONS.items():
            if callable(quantum_func):
                register_native_function(functions, quantum_func_name, quantum_func)
        register_native_function(functions, "quantum_qubit", lambda *args: quantum("qubit", *args))
        register_native_function(functions, "quantum_bell", lambda *args: quantum("bell", *args))
        register_native_function(functions, "quantum_measure", lambda *args, **kwargs: quantum("measure", *args, **kwargs))
        register_native_function(functions, "quantum_circuit", lambda *args: quantum("apply_circuit", *args))
        
        code.ok(f"{len(QUANTUM_FUNCTIONS)} funciones Quantum registradas como built-ins", module="quantum-registry")

    if TIMEWARP_ENABLED:
        for timewarp_func_name, timewarp_func in TIMEWARP_FUNCTIONS.items():
            if callable(timewarp_func):
                # Evitar conflictos de nombres con otros módulos
                if timewarp_func_name == "time_measure":
                    register_native_function(functions, "time_measure", timewarp_func)
                else:
                    register_native_function(functions, timewarp_func_name, timewarp_func)
        
        # Registrar aliases específicos para operaciones temporales
        register_native_function(functions, "timewarp_clock", lambda: timewarp("clock"))
        register_native_function(functions, "timewarp_timeline", lambda name="main": timewarp("timeline", name=name))
        register_native_function(functions, "timewarp_future", lambda delay, fn: timewarp("future", delay, fn))
        register_native_function(functions, "timewarp_measure", lambda fn: timewarp("measure", fn))
        
        code.ok(f"{len(TIMEWARP_FUNCTIONS)} funciones TimeWarp registradas como built-ins", module="timewarp-registry")

    if VISION_ENABLED:
        for vision_func_name, vision_func in VISION_FUNCTIONS.items():
            if callable(vision_func) and vision_func_name != "vision":
                register_native_function(functions, vision_func_name, vision_func)
        # Registrar función principal vision si existe
        if "vision" in VISION_FUNCTIONS:
            vision_main = VISION_FUNCTIONS["vision"]
            if isinstance(vision_main, dict):
                for func_name, func in vision_main.items():
                    if callable(func):
                        register_native_function(functions, f"vision_{func_name}", func)
        
        code.ok(f"{len(VISION_FUNCTIONS)} funciones Vision registradas como built-ins", module="vision-registry")


def eval_call_args(args, variables, functions):
    pos_args = []
    kw_args = {}
    for arg in args:
        if isinstance(arg, tuple) and arg[0] == "NAMED_ARG":
            name = arg[1]
            value = eval_expr(arg[2], variables, functions)
            kw_args[name] = to_native(value)
        else:
            pos_args.append(to_native(eval_expr(arg, variables, functions)))
    return pos_args, kw_args

def eval_expr(expr, variables, functions):
    # 1. Caso nulo
    if expr is None:
        return None

    # 2. Caso: referencia a variable ('IDENT', 'nombre_variable')
    if isinstance(expr, tuple) and len(expr) == 2 and expr[0] == "IDENT":
        _, name = expr
        # Agregar soporte para tipos básicos como identificadores
        if name in ("str", "int", "bool", "float"):
            return name  # Retorna el nombre del tipo como string
        if name in variables:
            print(f"DEBUG IDENT: {name} = {variables[name]} ({type(variables[name])})")
            return variables[name] 
        else:
            raise OrionRuntimeError(f"Variable '{name}' no definida")

    # 3. Caso: la expresión es una cadena y coincide con una variable existente
    if isinstance(expr, str) and expr in variables:
        val = variables[expr]
        if hasattr(val, "value"):
            return val.value
        return val

    # 4. Procesamiento de expresiones tipo tupla (AST)
    if isinstance(expr, tuple):
        tag = expr[0]

        # --- INDEX ---
        if tag == "INDEX":
            _, list_expr, index_expr = expr
            list_val = eval_expr(list_expr, variables, functions)
            index_val = eval_expr(index_expr, variables, functions)

            # Si es OrionList (o similar con .items)
            if isinstance(list_val, OrionList) or (hasattr(list_val, "items") and isinstance(getattr(list_val, "items"), (list, tuple))):
                container = getattr(list_val, "items", list_val)
                try:
                    return container[int(index_val)]
                except (ValueError, IndexError):
                    raise OrionRuntimeError(f"Índice fuera de rango o inválido: {index_val}")

            # Si es un wrapper que guarda value con lista interna (p.ej. OrionNumber/OrionString/otros)
            if hasattr(list_val, "value") and isinstance(getattr(list_val, "value"), (list, tuple, str)):
                inner = getattr(list_val, "value")
                try:
                    return inner[int(index_val)]
                except (ValueError, IndexError, TypeError):
                    raise OrionRuntimeError(f"Índice fuera de rango o inválido: {index_val}")

            # Si es lista o tupla nativa
            if isinstance(list_val, (list, tuple)):
                try:
                    return list_val[int(index_val)]
                except (ValueError, IndexError):
                    raise OrionRuntimeError(f"Índice fuera de rango o inválido: {index_val}")

            # --- FIX: Si es string pero esperabas una lista, corrige a lista vacía ---
            if isinstance(list_val, str):
                print(f"ADVERTENCIA: Se encontró string en una posición de lista, corrigiendo a lista vacía (valor era: {repr(list_val)})")
                return []

            # Si es diccionario
            if isinstance(list_val, dict):
                return list_val.get(index_val, None)

            # No indexable
            raise OrionRuntimeError(f"No se puede indexar el tipo {type(list_val).__name__}")
        # --- LIST ---
        elif tag == "LIST":
            _, elements = expr
            return [eval_expr(e, variables, functions) for e in elements]
        
        # --- LAMBDA ---
        elif tag == "LAMBDA":
            # Las lambdas se devuelven como están para ser procesadas por map/filter/etc
            return expr
        
        # --- DICT ---
        elif tag == "DICT":
            _, items = expr
            return {k: eval_expr(v, variables, functions) for k, v in items}

        # --- IDENT ---
        elif tag == "IDENT":
            _, name = expr
            # Agregar soporte para tipos básicos
            if name in ("str", "int", "bool", "float"):
                return name
            if name in variables:
                return variables[name] 
            else:
                raise OrionRuntimeError(f"Variable '{name}' no definida")
        
        # --- SLICE_ACCESS ---
        elif tag == "SLICE_ACCESS":
            _, obj_expr, slice_expr = expr
            obj_val = eval_expr(obj_expr, variables, functions)
            
            # Extract slice parameters
            _, start, end, step = slice_expr
            start_val = eval_expr(start, variables, functions) if start is not None else None
            end_val = eval_expr(end, variables, functions) if end is not None else None
            step_val = eval_expr(step, variables, functions) if step is not None else None
            
            # Get the actual object to slice
            if isinstance(obj_val, OrionList) or (hasattr(obj_val, "items") and isinstance(getattr(obj_val, "items"), (list, tuple))):
                container = getattr(obj_val, "items", obj_val)
            elif hasattr(obj_val, "value") and isinstance(getattr(obj_val, "value"), (list, tuple, str)):
                container = getattr(obj_val, "value")
            elif isinstance(obj_val, (list, tuple, str)):
                container = obj_val
            else:
                raise OrionRuntimeError(f"No se puede hacer slice del tipo {type(obj_val).__name__}")
            
            # Perform the slicing
            try:
                return container[start_val:end_val:step_val]
            except (TypeError, ValueError) as e:
                raise OrionRuntimeError(f"Error en slice: {str(e)}")

        # --- TYPE ---
        elif tag == "TYPE":
            _, type_name = expr
            return type_name  # Retorna el nombre del tipo como string

        # --- BINARY_OP ---
        elif tag == "BINARY_OP":
            _, op, left, right = expr
            left_val = eval_expr(left, variables, functions)
            right_val = eval_expr(right, variables, functions)
            print(f"DEBUG BINARY_OP: {op} | left={left_val} ({type(left_val)}) | right={right_val} ({type(right_val)})")

            # DEBUG: Mostrar qué estamos comparando
            # print(f"DEBUG BINARY_OP: {op} entre {type(left_val).__name__}({left_val}) y {type(right_val).__name__}({right_val})")

            # if hasattr(left_val, "value"):
            #     left_val = left_val.value
            # if hasattr(right_val, "value"):
            #     right_val = right_val.value

            # Intentar convertir strings numéricos a enteros o flotantes
            def try_cast_numeric(v):
                if isinstance(v, str):
                    try:
                        if "." in v:
                            return float(v)
                        return int(v)
                    except ValueError:
                        return v
                return v

             # --- OPERADORES BINARIOS ---
            if op == "+":
                # Si alguno es OrionString, resultado OrionString
                if isinstance(left_val, OrionString) or isinstance(right_val, OrionString):
                    return OrionString(str(left_val) + str(right_val))
                # Si alguno es OrionNumber, resultado OrionNumber
                if isinstance(left_val, OrionNumber) or isinstance(right_val, OrionNumber):
                    return left_val + right_val  # <--- sin OrionNumber(...)
                # Si alguno es string, resultado string
                if isinstance(left_val, str) or isinstance(right_val, str):
                    return str(left_val) + str(right_val)
                return left_val + right_val

            elif op == "-":
                if isinstance(left_val, OrionNumber) or isinstance(right_val, OrionNumber):
                    return left_val - right_val
                return left_val - right_val

            elif op == "*":
                if isinstance(left_val, OrionNumber) or isinstance(right_val, OrionNumber):
                    return left_val * right_val
                return left_val * right_val

            elif op == "**":
                if isinstance(left_val, OrionNumber) or isinstance(right_val, OrionNumber):
                    return left_val ** right_val
                return left_val ** right_val

            elif op == "/":
                if isinstance(left_val, OrionNumber) or isinstance(right_val, OrionNumber):
                    return left_val / right_val
                return left_val / right_val

            elif op == "%":
                if isinstance(left_val, OrionNumber) or isinstance(right_val, OrionNumber):
                    return left_val % right_val
                return left_val % right_val

            elif op in [">", "<", ">=", "<=", "==", "!="]:
                # MEJORADO: Manejo especial para comparaciones con tipos
                if op in ["==", "!="]:
                    # Función para normalizar nombres de tipo
                    def normalize_type_name(val):
                        if isinstance(val, str):
                            if val in ["list", "string", "number", "bool", "dict"]:
                                return val
                            return val
                        elif hasattr(val, '__class__'):
                            class_name = val.__class__.__name__
                            if class_name == 'OrionList' or isinstance(val, list):
                                return "list"
                            elif class_name == 'OrionString' or isinstance(val, str):
                                return "string"
                            elif class_name == 'OrionNumber' or isinstance(val, (int, float)):
                                return "number"
                            elif class_name == 'OrionBool' or isinstance(val, bool):
                                return "bool"
                            elif class_name == 'OrionDict' or isinstance(val, dict):
                                return "dict"
                            else:
                                return class_name.lower()
                        return str(type(val).__name__).lower()
                    
                    left_type = normalize_type_name(left_val)
                    right_type = normalize_type_name(right_val)
                    
                    # print(f"DEBUG TYPE COMPARISON: '{left_type}' {op} '{right_type}'")
                    
                    if (left_type in ["list", "string", "number", "bool", "dict"] or 
                        right_type in ["list", "string", "number", "bool", "dict"]):
                        result = (left_type == right_type) if op == "==" else (left_type != right_type)
                        # print(f"DEBUG TYPE RESULT: {result}")
                        return result

                # FIXED: Comparaciones de caracteres/strings - MOVER ANTES de try_cast_numeric
                if isinstance(left_val, str) and isinstance(right_val, str):
                    # Para caracteres individuales, usar comparación ASCII
                    if len(left_val) == 1 and len(right_val) == 1:
                        left_ord = ord(left_val)
                        right_ord = ord(right_val)
                        if op == ">": return left_ord > right_ord
                        if op == "<": return left_ord < right_ord
                        if op == ">=": return left_ord >= right_ord
                        if op == "<=": return left_ord <= right_ord
                        if op == "==": return left_ord == right_ord
                        if op == "!=": return left_ord != right_ord
                    else:
                        # Para strings más largos, usar comparación lexicográfica
                        if op == ">": return left_val > right_val
                        if op == "<": return left_val < right_val
                        if op == ">=": return left_val >= right_val
                        if op == "<=": return left_val <= right_val
                        if op == "==": return left_val == right_val
                        if op == "!=": return left_val != right_val

                # Intentar normalizar tipos antes de comparar (solo si no es comparación de tipos y no son strings)
                left_val = try_cast_numeric(left_val)
                right_val = try_cast_numeric(right_val)

                # Comparaciones numéricas
                if isinstance(left_val, (int, float)) and isinstance(right_val, (int, float)):
                    if op == ">": return left_val > right_val
                    if op == "<": return left_val < right_val
                    if op == ">=": return left_val >= right_val
                    if op == "<=": return left_val <= right_val
                    if op == "==": return left_val == right_val
                    if op == "!=": return left_val != right_val

                # Si uno es string numérico y otro número (después de try_cast_numeric)
                if isinstance(left_val, str) and left_val.replace('.', '', 1).isdigit():
                    left_val = float(left_val) if '.' in left_val else int(left_val)
                    return eval_expr(("BINARY_OP", op, left_val, right_val), variables, functions)
                if isinstance(right_val, str) and right_val.replace('.', '', 1).isdigit():
                    right_val = float(right_val) if '.' in right_val else int(right_val)
                    return eval_expr(("BINARY_OP", op, left_val, right_val), variables, functions)

                # --- FIX: Si uno es string no numérico y otro es número, retorna False para comparación de orden ---
                if (isinstance(left_val, (int, float)) and isinstance(right_val, str)) or (isinstance(right_val, (int, float)) and isinstance(left_val, str)):
                    if op in [">", "<", ">=", "<="]:
                        return False
                    if op == "==":
                        return str(left_val) == str(right_val)
                    if op == "!=":
                        return str(left_val) != str(right_val)

                print(f"DEBUG FALLBACK: No se puede comparar {type(left_val).__name__} con {type(right_val).__name__}")
                raise OrionRuntimeError(
                    f"No se puede comparar {type(left_val).__name__} con {type(right_val).__name__}"
                )

            # --- FIX DEFINITIVO: operadores lógicos (&& y ||) ---
            elif op in ["&&", "||"]:
                # Extrae el valor interno de OrionBool, OrionNumber, etc.
                if hasattr(left_val, "value"):
                    left_val = left_val.value
                if hasattr(right_val, "value"):
                    right_val = right_val.value

                left_bool = bool(left_val)
                right_bool = bool(right_val)

                # print(f"DEBUG LOGICAL_OP: {op} entre {left_bool} y {right_bool}")

                if op == "&&":
                    return left_bool and right_bool
                else:
                    return left_bool or right_bool

            else:
                raise OrionRuntimeError(f"Operador binario desconocido: {op}")

        # --- UNARY_OP ---
        elif tag == "UNARY_OP":
            _, op, operand = expr
            operand_val = eval_expr(operand, variables, functions)
            
            if op == "!":
                return not operand_val
            elif op == "-":
                return -operand_val
            elif op == "+":
                return +operand_val
            else:
                raise OrionRuntimeError(f"Operador unario desconocido: {op}")

        # --- CALL ---
        elif tag == "CALL":
            if len(expr) == 4:
                _, fn_name, args, kwargs = expr  
            elif len(expr) == 3:
                _, fn_name, args = expr
                kwargs = {}
            else:
                raise OrionRuntimeError(f"Formato de llamada de función desconocido: {expr}")
            
            if isinstance(fn_name, tuple) and fn_name[0] == "IDENT":
                fn_name = fn_name[1]
            
            # Filtrar kwargs para funciones visuales que no los aceptan
            if fn_name in ["trace_start", "trace_end", "frame", "divider"]:
                fn_def = get_function(functions, fn_name)
                if fn_def is None and fn_name in NATIVE_FUNCTIONS:
                    fn_def = {
                        "type": "NATIVE_FN",
                        "impl": NATIVE_FUNCTIONS[fn_name]
                    }
                if not fn_def:
                    raise OrionFunctionError(f"Función no definida: {fn_name}")
                pos_args, _ = eval_call_args(args, variables, functions)
                return fn_def["impl"](*pos_args[:1])

            # MANEJO ESPECIAL PARA APPEND SIN OBJETO (ERROR DE PARSING)
            if fn_name == "append":
                if len(args) == 1:
                    arg_val = eval_expr(args[0], variables, functions)
                    
                    # Estrategia 1: Buscar variable que termine en "_titles" o similar
                    target_var = None
                    target_list = None
                    
                    for var_name, var_val in variables.items():
                        if var_name.endswith("_titles") or var_name.endswith("titles"):
                            if isinstance(var_val, list):
                                target_var = var_name
                                target_list = var_val
                                break
                            elif hasattr(var_val, 'items') and isinstance(var_val.items, list):
                                target_var = var_name
                                target_list = var_val.items
                                break
                            elif hasattr(var_val, 'value') and isinstance(var_val.value, list):
                                target_var = var_name
                                target_list = var_val.value
                                break
                    
                    # Estrategia 2: Si no se encuentra, usar la lista más reciente
                    if target_list is None:
                        for var_name, var_val in reversed(list(variables.items())):
                            if isinstance(var_val, list):
                                target_var = var_name
                                target_list = var_val
                                break
                            elif hasattr(var_val, 'items') and isinstance(var_val.items, list):
                                target_var = var_name
                                target_list = var_val.items
                                break
                            elif hasattr(var_val, 'value') and isinstance(var_val.value, list):
                                target_var = var_name
                                target_list = var_val.value
                                break
                    
                    if target_list is not None:
                        target_list.append(arg_val)
                        return None  # append no retorna valor
                    else:
                        raise OrionFunctionError("append() llamado sin objeto - no se encontró lista válida")
                else:
                    raise OrionFunctionError("append() requiere exactamente 1 argumento")

            fn_def = get_function(functions, fn_name)
            if fn_def is None and fn_name in NATIVE_FUNCTIONS:
                fn_def = {
                    "type": "NATIVE_FN",
                    "impl": NATIVE_FUNCTIONS[fn_name]
                }
            if not fn_def:
                raise OrionFunctionError(f"Función no definida: {fn_name}")

            pos_args, kw_args = eval_call_args(args, variables, functions)
            if kwargs:
                for k, v in kwargs.items():
                    kw_args[k] = to_native(eval_expr(v, variables, functions))

            # Función nativa
            if fn_def["type"] == "NATIVE_FN":
                # === MANEJO ESPECIAL PARA FUNCIONES SPREADSHEET ===
                if SPREADSHEET_ENABLED and (fn_name in SPREADSHEET_FUNCTIONS or fn_name.startswith('sheet_') or 
                                          fn_name.startswith('sync_') or fn_name.startswith('osync_') or
                                          fn_name in ["create_sheet", "attach_sheet", "register_sheet", "link_sheet", "push_remote", "pull_remote"]):
                    try:
                        result = fn_def["impl"](*pos_args, **kw_args)
                        if fn_name in ["create_sheet", "attach_sheet", "sync_push", "sync_pull", "osync_execute"]:
                            code.info(f"[DEBUG SPREADSHEET] Ejecutado {fn_name} con {len(pos_args)} argumentos")
                        return result
                    except Exception as e:
                        raise OrionRuntimeError(f"Error en función Spreadsheet '{fn_name}': {str(e)}")

                # === MANEJO ESPECIAL PARA FUNCIONES AI ===
                if AI_ENABLED and (fn_name in AI_FUNCTIONS or fn_name.startswith('ai_')):
                    try:
                        result = fn_def["impl"](*pos_args, **kw_args)
                        # Registrar la operación AI en memoria si es think/embed/recall
                        if fn_name in ["think", "ai_think", "embed", "ai_embed"]:
                            code.info(f"[DEBUG AI] Ejecutado {fn_name} con {len(pos_args)} argumentos")
                        return result
                    except Exception as e:
                        raise OrionRuntimeError(f"Error en función AI '{fn_name}': {str(e)}")
                
                # === MANEJO ESPECIAL PARA FUNCIONES COSMOS ===
                elif COSMOS_ENABLED and (fn_name in COSMOS_FUNCTIONS or fn_name.startswith('cosmos_')):
                    try:
                        result = fn_def["impl"](*pos_args, **kw_args)
                        if fn_name in ["cosmos", "cosmos_create", "cosmos_run"]:
                            code.info(f"[DEBUG COSMOS] Ejecutado {fn_name} con {len(pos_args)} argumentos")
                        return result
                    except Exception as e:
                        raise OrionRuntimeError(f"Error en función Cosmos '{fn_name}': {str(e)}")
                
                # === MANEJO ESPECIAL PARA FUNCIONES CRYPTO ===
                elif CRYPTO_ENABLED and (fn_name in CRYPTO_FUNCTIONS or fn_name.startswith('crypto_')):
                    try:
                        result = fn_def["impl"](*pos_args, **kw_args)
                        if fn_name in ["crypto", "hash", "encrypt", "decrypt", "sign", "verify"]:
                            code.info(f"[DEBUG CRYPTO] Ejecutado {fn_name} con {len(pos_args)} argumentos")
                        return result
                    except Exception as e:
                        raise OrionRuntimeError(f"Error en función Crypto '{fn_name}': {str(e)}")
                
                # === MANEJO ESPECIAL PARA FUNCIONES INSIGHT ===
                elif INSIGHT_ENABLED and (fn_name in INSIGHT_FUNCTIONS or fn_name.startswith('insight_')):
                    try:
                        result = fn_def["impl"](*pos_args, **kw_args)
                        if fn_name in ["extract_text_blocks", "extract_tables", "extract_metadata", "extract_signatures", "summarize"]:
                            code.info(f"[DEBUG INSIGHT] Ejecutado {fn_name} con {len(pos_args)} argumentos")
                        return result
                    except Exception as e:
                        raise OrionRuntimeError(f"Error en función Insight '{fn_name}': {str(e)}")
                
                # === MANEJO ESPECIAL PARA FUNCIONES MATRIX ===
                elif MATRIX_ENABLED and (fn_name in MATRIX_FUNCTIONS or fn_name.startswith('matrix_')):
                    try:
                        result = fn_def["impl"](*pos_args, **kw_args)
                        if fn_name in ["matrix_add", "matrix_mul", "matrix_det", "matrix_inv"]:
                            code.info(f"[DEBUG MATRIX] Ejecutado {fn_name} con {len(pos_args)} argumentos")
                        return result
                    except Exception as e:
                        raise OrionRuntimeError(f"Error en función Matrix '{fn_name}': {str(e)}")

                # === MANEJO ESPECIAL PARA FUNCIONES QUANTUM ===
                if QUANTUM_ENABLED and (fn_name in QUANTUM_FUNCTIONS or fn_name.startswith('quantum_') or
                                        fn_name in ["qubit", "bell_pair", "measure", "apply_gate", "tensor", "fidelity"]):
                    try:
                        result = fn_def["impl"](*pos_args, **kw_args)
                        if fn_name in ["quantum", "qubit", "bell_pair", "measure", "apply_circuit"]:
                            code.info(f"[DEBUG QUANTUM] Ejecutado {fn_name} - operación cuántica completada")
                        return result
                    except Exception as e:
                        raise OrionRuntimeError(f"Error en función Quantum '{fn_name}': {str(e)}")
                
                # === MANEJO ESPECIAL PARA FUNCIONES TIMEWARP ===
                if TIMEWARP_ENABLED and (fn_name in TIMEWARP_FUNCTIONS or fn_name.startswith('timewarp_') or 
                                       fn_name in ["WarpClock", "TimeLine", "future", "warp_speed", "wait", "time_measure"]):
                    try:
                        result = fn_def["impl"](*pos_args, **kw_args)
                        if fn_name in ["timewarp", "WarpClock", "future", "wait", "time_measure"]:
                            code.info(f"[DEBUG TIMEWARP] Ejecutado {fn_name} - operación temporal completada")
                        return result
                    except Exception as e:
                        raise OrionRuntimeError(f"Error en función TimeWarp '{fn_name}': {str(e)}")

                # === MANEJO ESPECIAL PARA FUNCIONES VISION ===
                if VISION_ENABLED and (fn_name in VISION_FUNCTIONS or fn_name.startswith('vision_') or 
                                     fn_name in ["load", "save", "resize", "smart_crop", "dhash", "detect_faces", "blur_faces", "ImagePipeline"]):
                    try:
                        result = fn_def["impl"](*pos_args, **kw_args)
                        if fn_name in ["load", "save", "resize", "smart_crop", "detect_faces", "blur_faces"]:
                            code.info(f"[DEBUG VISION] Ejecutado {fn_name} - procesamiento de imagen completado")
                        return result
                    except Exception as e:
                        raise OrionRuntimeError(f"Error en función Vision '{fn_name}': {str(e)}")

                # Procesar argumentos especialmente para show
                elif fn_name == "show":
                    processed_args = []
                    for i, arg in enumerate(args):
                        if isinstance(arg, str) and arg.startswith('"') and arg.endswith('"'):
                            raw = arg[1:-1]  # Quitar comillas
                            orion_str = OrionString(raw)
                            interpolated = orion_str.interpolate(variables)
                            processed_args.append(str(interpolated))
                        else:
                            processed_args.append(pos_args[i])
                    # Usar show con capacidades extendidas
                    return fn_def["impl"](*processed_args, **kw_args)
                else:
                    pos_args = [str(a) if isinstance(a, OrionString) else a for a in pos_args]
                    return fn_def["impl"](*pos_args, **kw_args)

            # Función definida por usuario
            elif fn_def["type"] == "FN_DEF":
                params = fn_def.get("params", [])
                body = fn_def.get("body", [])
                if len(pos_args) != len(params):
                    raise OrionFunctionError(
                        f"Argumentos esperados: {len(params)}, recibidos: {len(pos_args)}"
                    )
                local_vars = variables.copy()
                for p, a in zip(params, pos_args):
                    local_vars[p] = a
                local_vars.update(kw_args)
                return evaluate(body, local_vars, functions, inside_fn=True)
            
            else:
                raise OrionFunctionError(f"Tipo de función desconocido: {fn_def.get('type', 'UNKNOWN')}")

        # --- CALL_METHOD ---
        elif tag == "CALL_METHOD":
            # Soporta ambos formatos: 4 o 5 elementos
            if len(expr) == 5:
                _, method_name, obj_expr, args, kwargs = expr
            elif len(expr) == 4:
                _, method_name, obj_expr, args = expr
                kwargs = {}
            else:
                raise OrionRuntimeError(f"Formato de llamada a método desconocido: {expr}")

            obj_val = eval_expr(obj_expr, variables, functions)
            pos_args, kw_args = eval_call_args(args, variables, functions)
            if kwargs:
                for k, v in kwargs.items():
                    kw_args[k] = to_native(eval_expr(v, variables, functions))
            if method_name == "isdigit":
                if isinstance(obj_val, int):
                    return True
                if isinstance(obj_val, str):
                    return obj_val.isdigit()
                raise OrionFunctionError(f"Método 'isdigit' no disponible para tipo {type(obj_val).__name__}")
            
            if method_name == "upper":
                # CORREGIDO: Manejo más robusto de upper()
                if isinstance(obj_val, str) and len(obj_val) > 0:
                    return obj_val.upper()
                elif hasattr(obj_val, "value") and isinstance(obj_val.value, str) and len(str(obj_val.value)) > 0:
                    return str(obj_val.value).upper()
                elif isinstance(obj_val, list) and len(obj_val) > 0:
                    # Si es lista, tomar el primer elemento y convertir a string
                    first_item = obj_val[0]
                    if isinstance(first_item, str) and len(first_item) > 0:
                        return first_item.upper()
                    else:
                        str_val = str(first_item)
                        return str_val.upper() if len(str_val) > 0 else ""
                elif hasattr(obj_val, "items") and isinstance(obj_val.items, list) and len(obj_val.items) > 0:
                    # Si es OrionList, tomar el primer elemento
                    first_item = obj_val.items[0]
                    if isinstance(first_item, str) and len(first_item) > 0:
                        return first_item.upper()
                    else:
                        str_val = str(first_item)
                        return str_val.upper() if len(str_val) > 0 else ""
                else:
                    # FALLBACK: Convertir a string y aplicar upper, verificando longitud
                    str_val = str(obj_val)
                    return str_val.upper() if len(str_val) > 0 else ""
            
            elif method_name == "items":
                # ✅ ORION ADVANCED ITERATOR SAFETY SYSTEM - FINAL FIX
                cache_key = f"_items_cache_{id(obj_val)}"

                # 1. Global cache (más importante)
                if hasattr(eval_expr, '_dict_cache') and id(obj_val) in eval_expr._dict_cache:
                    return eval_expr._dict_cache[id(obj_val)]

                # 2. Objeto ya tiene snapshot cacheado
                if hasattr(obj_val, cache_key):
                    return getattr(obj_val, cache_key)

                # 3. Diccionario nativo
                if isinstance(obj_val, dict):
                    result = list(obj_val.items())
                    if not hasattr(eval_expr, '_dict_cache'):
                        eval_expr._dict_cache = {}
                    eval_expr._dict_cache[id(obj_val)] = result
                    return result

                # 4. Diccionario envuelto (OrionDict, etc.)
                elif hasattr(obj_val, 'value') and isinstance(obj_val.value, dict):
                    result = list(obj_val.value.items())
                    if not hasattr(eval_expr, '_dict_cache'):
                        eval_expr._dict_cache = {}
                    eval_expr._dict_cache[id(obj_val)] = result
                    try:
                        setattr(obj_val, cache_key, result)
                    except (AttributeError, TypeError):
                        pass
                    return result

                # 5. Objeto con método items() (proteger recursión)
                elif hasattr(obj_val, 'items') and callable(obj_val.items):
                    if hasattr(obj_val, '_processing_items'):
                        # Recursión detectada, usar caché o lista vacía
                        if hasattr(eval_expr, '_dict_cache') and id(obj_val) in eval_expr._dict_cache:
                            return eval_expr._dict_cache[id(obj_val)]
                        return []
                    try:
                        obj_val._processing_items = True
                        if not hasattr(eval_expr, '_dict_cache'):
                            eval_expr._dict_cache = {}
                        eval_expr._dict_cache[id(obj_val)] = []
                        result = obj_val.items()
                        # Siempre snapshot como lista
                        cached_result = list(result) if hasattr(result, '__iter__') and not isinstance(result, (str, bytes)) else result
                        eval_expr._dict_cache[id(obj_val)] = cached_result
                        try:
                            setattr(obj_val, cache_key, cached_result)
                        except (AttributeError, TypeError):
                            pass
                        return cached_result
                    except RecursionError as e:
                        code.error(f"Deep recursion in items() - using emergency cache: {str(e)}", module="iterator-safety")
                        return eval_expr._dict_cache.get(id(obj_val), [])
                    except Exception as e:
                        code.warn(f"Error in items() call: {str(e)} - using emergency cache", module="iterator-safety")
                        return eval_expr._dict_cache.get(id(obj_val), [])
                    finally:
                        if hasattr(obj_val, '_processing_items'):
                            delattr(obj_val, '_processing_items')

                else:
                    raise OrionFunctionError(f"Método 'items' no disponible para tipo {type(obj_val).__name__}")

    
            # === MÉTODOS BUILT-IN COMUNES ===
            if method_name == "len":
                if hasattr(obj_val, '__len__'):
                    return len(obj_val)
                elif isinstance(obj_val, (list, dict, str, tuple)):
                    return len(obj_val)
                elif hasattr(obj_val, 'items') and isinstance(obj_val.items, (list, tuple)):
                    return len(obj_val.items)
                elif hasattr(obj_val, 'value'):
                    inner = obj_val.value
                    if hasattr(inner, '__len__'):
                        return len(inner)
                    elif isinstance(inner, (list, dict, str, tuple)):
                        return len(inner)
                else:
                    raise OrionFunctionError(f"Objeto de tipo {type(obj_val)} no tiene longitud calculable")
            
            elif method_name == "filter":
                if len(pos_args) != 1:
                    raise OrionFunctionError("filter() requiere exactamente 1 argumento")
                lambda_expr = args[0]
                # Si es una lambda AST, envolverla en función Python
                if isinstance(lambda_expr, tuple) and lambda_expr[0] == "LAMBDA":
                    _, params, body = lambda_expr
                    def fn(*lambda_args):
                        local_scope = variables.copy()
                        for i, param in enumerate(params):
                            if i < len(lambda_args):
                                local_scope[param] = lambda_args[i]
                            else:
                                local_scope[param] = None
                        # Si algún parámetro es None y se usa como índice, retorna False
                        if any(local_scope[p] is None for p in params):
                            return False
                        return eval_expr(body, local_scope, functions)
                else:
                    fn = lambda_expr 

                # Determinar la colección a filtrar
                if isinstance(obj_val, list):
                    return [x for x in obj_val if fn(x)]
                elif hasattr(obj_val, 'items') and isinstance(obj_val.items, list):
                    return obj_val.__class__([x for x in obj_val.items if fn(x)])
                elif hasattr(obj_val, 'value') and isinstance(obj_val.value, list):
                    return obj_val.__class__([x for x in obj_val.value if fn(x)])
                else:
                    raise OrionFunctionError(f"Método 'filter' no disponible para tipo {type(obj_val)}")
    
            elif method_name == "append":
                if len(pos_args) == 0:
                    raise OrionFunctionError("append() requiere al menos 1 argumento")

                # Asegurar que sea lista nativa, no wrapper ni string accidental
                if isinstance(obj_val, str):
                    raise OrionFunctionError(f"No se puede usar append() sobre tipo str ({obj_val})")

                if isinstance(obj_val, list):
                    for arg in pos_args:
                        obj_val.append(arg)
                    return obj_val  # mantener referencia de lista

                elif hasattr(obj_val, 'items') and isinstance(obj_val.items, list):
                    for arg in pos_args:
                        obj_val.items.append(arg)
                    return obj_val

                elif hasattr(obj_val, 'value') and isinstance(obj_val.value, list):
                    for arg in pos_args:
                        obj_val.value.append(arg)
                    return obj_val

                elif hasattr(obj_val, "append") and callable(getattr(obj_val, "append")):
                    for arg in pos_args:
                        obj_val.append(arg)
                    return obj_val  # devolver siempre el objeto, no None

                else:
                    raise OrionFunctionError(f"Método 'append' no disponible para tipo {type(obj_val)}")
            
            elif method_name == "join":
                if len(pos_args) != 1:
                    raise OrionFunctionError("join() requiere exactamente 1 argumento")
                separator = pos_args[0]
                if isinstance(obj_val, list):
                    return separator.join(str(item) for item in obj_val)
                elif hasattr(obj_val, 'items') and isinstance(obj_val.items, list):
                    return separator.join(str(item) for item in obj_val.items)
                elif hasattr(obj_val, 'value') and isinstance(obj_val.value, list):
                    return separator.join(str(item) for item in obj_val.value)
                else:
                    raise OrionFunctionError(f"Método 'join' no disponible para tipo {type(obj_val)}")
            
            elif method_name == "keys":
                if isinstance(obj_val, dict):
                    return list(obj_val.keys())
                elif hasattr(obj_val, 'value') and isinstance(obj_val.value, dict):
                    return list(obj_val.value.keys())
                else:
                    raise OrionFunctionError(f"Método 'keys' no disponible para tipo {type(obj_val)}")
            
            elif method_name == "map":
                if len(pos_args) != 1:
                    raise OrionFunctionError("map() requiere exactamente 1 argumento")
                lambda_expr = args[0]
                # Determinar la colección a mapear
                if isinstance(obj_val, list):
                    collection = obj_val
                elif hasattr(obj_val, 'items') and isinstance(obj_val.items, list):
                    collection = obj_val.items
                elif hasattr(obj_val, 'value') and isinstance(obj_val.value, list):
                    collection = obj_val.value
                else:
                    raise OrionFunctionError(f"Método 'map' no disponible para tipo {type(obj_val)}")
                # Si es una lambda AST, envolverla en función Python
                if isinstance(lambda_expr, tuple) and lambda_expr[0] == "LAMBDA":
                    _, params, body = lambda_expr
                    result = []
                    for item in collection:
                        local_scope = variables.copy()
                        # Si hay varios parámetros, desempaquetar item si es tupla/lista
                        if len(params) == 1:
                            local_scope[params[0]] = item
                        else:
                            # Rellenar con None si faltan elementos
                            if isinstance(item, (list, tuple)):
                                for i, param in enumerate(params):
                                    if i < len(item):
                                        local_scope[param] = item[i]
                                    else:
                                        local_scope[param] = None
                            else:
                                for param in params:
                                    local_scope[param] = item
                        # Si algún parámetro es None y se usa como índice, puedes retornar None o saltar
                        mapped_value = eval_expr(body, local_scope, functions)
                        result.append(mapped_value)
                    return result
                else:
                    raise OrionFunctionError("map() requiere una función lambda (usando =>)")
                
            # === MÉTODOS ESPECÍFICOS DE MÓDULOS SPREADSHEET ===
            elif SPREADSHEET_ENABLED and method_name in ["write", "read", "save", "sync", "push", "pull", "register", "attach", "link"]:
                try:
                    if method_name == "write":
                        # Para sheet.write(cell, value)
                        if len(pos_args) >= 2:
                            cell = pos_args[0]
                            value = pos_args[1]
                            obj_val.write(cell, value)
                            return obj_val
                        else:
                            raise OrionFunctionError("write() requiere al menos 2 argumentos (cell, value)")
                    
                    elif method_name == "read":
                        # Para sheet.read(cell)
                        if len(pos_args) >= 1:
                            cell = pos_args[0]
                            return obj_val.read(cell)
                        else:
                            raise OrionFunctionError("read() requiere al menos 1 argumento (cell)")
                    
                    elif method_name == "save":
                        # Para sheet.save()
                        return obj_val.save()
                    
                    elif method_name == "sync":
                        # Para sheet.sync() o sheet.sync(endpoint)
                        if len(pos_args) >= 1:
                            endpoint = pos_args[0]
                            return obj_val.sync(endpoint)
                        else:
                            return obj_val.sync("default")
                    
                    elif method_name in ["push", "pull"]:
                        # Para OSync.push() o OSync.pull()
                        if hasattr(obj_val, method_name):
                            method = getattr(obj_val, method_name)
                            return method(*pos_args, **kw_args)
                        else:
                            raise OrionFunctionError(f"Método '{method_name}' no disponible en este objeto")
                    
                    elif method_name in ["register", "attach", "link"]:
                        # Para métodos estáticos de las clases
                        if hasattr(obj_val, method_name):
                            method = getattr(obj_val, method_name)
                            return method(*pos_args, **kw_args)
                        else:
                            raise OrionFunctionError(f"Método '{method_name}' no disponible en este objeto")
                    
                    else:
                        # Para otros métodos Spreadsheet
                        if hasattr(obj_val, method_name):
                            method = getattr(obj_val, method_name)
                            if callable(method):
                                return method(*pos_args, **kw_args)
                            else:
                                return method
                        else:
                            raise OrionFunctionError(f"Método Spreadsheet '{method_name}' no encontrado")

                except Exception as e:
                    raise OrionRuntimeError(f"Error en método Spreadsheet '{method_name}': {str(e)}")

            # === MÉTODOS ESPECÍFICOS DE MÓDULOS AI/OTROS ===
            elif AI_ENABLED and method_name in ["embed", "think", "fit", "predict", "cluster", "sim", "dist", "normalize", "accuracy", "mse"]:
                try:
                    if method_name == "embed":
                        # Para ai.embed(), obj_val es el objeto ai, pos_args[0] es el texto
                        if len(pos_args) >= 1: 
                            text = pos_args[0]
                            dim = pos_args[1] if len(pos_args) > 1 else 128
                            return quantum_embed(text, dim)
                        else:
                            raise OrionFunctionError("embed() requiere al menos un argumento (texto)")
                            
                    elif method_name == "cluster":
                        ai_func = AI_FUNCTIONS.get("cluster")
                        if ai_func:
                            # Filtrar kwargs para que solo pasen los soportados por la función
                            sig = inspect.signature(ai_func)
                            supported = {k: v for k, v in kw_args.items() if k in sig.parameters}
                            centers, labels = ai_func(*pos_args, **supported)
                            if isinstance(labels, OrionList):
                                labels = [int(x) for x in labels.items]
                            else:
                                labels = [int(x) for x in labels]
                            return [centers, labels]
                        else:
                            raise OrionFunctionError(f"Método AI 'cluster' no encontrado")

                    elif method_name in ["think", "fit", "predict", "cluster", "embed"]:
                        ai_func = get_function(functions, method_name)
                        # --- FIX: Si ai_func es dict con 'impl', extraer la función ---
                        if isinstance(ai_func, dict) and "impl" in ai_func:
                            ai_func = ai_func["impl"]
                        if ai_func:
                            sig = inspect.signature(ai_func)
                            supported = {k: v for k, v in kw_args.items() if k in sig.parameters}
                            return ai_func(*pos_args, **supported)
                        else:
                            raise OrionFunctionError(f"Método AI '{method_name}' no encontrado")
                    else:
                        # Para otros métodos AI
                        ai_func = AI_FUNCTIONS.get(method_name)
                        if ai_func:
                            return ai_func(*pos_args, **kw_args)
                        else:
                            raise OrionFunctionError(f"Método AI '{method_name}' no encontrado")
                except Exception as e:
                    raise OrionRuntimeError(f"Error en método AI '{method_name}': {str(e)}")

            # === FALLBACK A lib.math ===
            elif hasattr(orion_math, method_name):
                fn = getattr(orion_math, method_name)
                return fn(obj_val, *pos_args, **kw_args)
            
            # === MÉTODO NATIVO DEL OBJETO ===
            elif hasattr(obj_val, method_name):
                method = getattr(obj_val, method_name)
                if callable(method):
                    return method(*pos_args, **kw_args)
                else:
                    return method
            
            else:
                raise OrionFunctionError(
                    f"Método '{method_name}' no definido para objeto de tipo {type(obj_val).__name__}"
                )

        # --- ATTR_ACCESS ---
        elif tag == "ATTR_ACCESS":
            _, obj_expr, attr_name = expr
            obj_val = eval_expr(obj_expr, variables, functions)
            # Si es una instancia de clase, accede al atributo directamente
            if hasattr(obj_val, attr_name):
                val = getattr(obj_val, attr_name)
                # Si el valor es una propiedad o método, evalúalo si es necesario
                if callable(val):
                    return val()
                return val
            # Si es un diccionario
            elif isinstance(obj_val, dict) and attr_name in obj_val:
                return obj_val[attr_name]
            else:
                raise OrionRuntimeError(f"Atributo '{attr_name}' no encontrado en objeto.")

        # --- NULL_SAFE ---
        elif tag == "NULL_SAFE":
            # NULL_SAFE: ('NULL_SAFE', object_expr, attr_name)
            _, object_expr, attr_name = expr
            obj = eval_expr(object_expr, variables, functions)
            
            # Si el objeto es null/None, devolver null
            if obj is None:
                return None
            
            # Si el objeto tiene el atributo, devolverlo
            if hasattr(obj, attr_name):
                return getattr(obj, attr_name)
            elif isinstance(obj, dict) and attr_name in obj:
                return obj[attr_name]
            else:
                return None

    # 5. Tipos básicos
    if isinstance(expr, bool):
        return expr

    if isinstance(expr, (int, float)):
        return expr

    if isinstance(expr, str):
        if expr.startswith('"') and expr.endswith('"'):
            content = expr[1:-1]  # Quitar comillas
            # Interpolación de variables ${variable}
            import re
            def replace_var(match):
                var_name = match.group(1)
                if var_name in variables:
                    val = variables[var_name]
                    if hasattr(val, 'value'):
                        return str(val.value)
                    return str(val)
                return match.group(0)  # Si no existe la variable, dejar como está
            
            interpolated = re.sub(r'\$\{(\w+)\}', replace_var, content)
            return interpolated
        if expr in ("true", "yes"):
            return True
        if expr in ("false", "no"):
            return False
        if expr.isdigit():
            return int(expr)
        if expr.replace('.', '', 1).isdigit():
            return float(expr)
        if expr.startswith('"') and expr.endswith('"'):
            raw = expr.strip('"')
            # Crear OrionString, interpolar y retornar el valor interpolado
            orion_str = OrionString(raw)
            interpolated = orion_str.interpolate(variables)
            return str(interpolated)  # Convertir a string nativo
        if expr in variables:
            return lookup_variable(expr, variables)
        return expr

    # 6. Si no coincide con nada
    return expr

def evaluate(ast, variables=None, functions=None, inside_fn=False):
    if variables is None:
        variables = {}
    if functions is None:
        functions = {}

    # Solo inicializar el motor visual si NO estamos dentro de una función Y es la primera llamada
    if not inside_fn and not hasattr(evaluate, '_initialized'):
        # === INICIALIZAR ORION VISUAL ENGINE ===
        code.frame("ORION LANGUAGE CORE", style="cyan")
        code.divider("System Initialization")

        # === Inicializar valores nativos de Orion ===
        if "null" not in variables:
            variables["null"] = None
        if "yes" not in variables:
            variables["yes"] = OrionBool(True)
        if "no" not in variables:
            variables["no"] = OrionBool(False)
        if "clusters" not in variables:
            variables["clusters"] = {}
            
        # === INICIALIZAR CONTEXTOS (solo una vez) ===
        if SPREADSHEET_ENABLED:
            variables["SPREADSHEET"] = {
                "enabled": True,
                "functions": list(SPREADSHEET_FUNCTIONS.keys()),
                "version": "1.0.0",
                "features": ["xlsx", "csv", "orion_sheets", "local_sync", "remote_sync", "osync_protocol"]
            }
            code.info("Spreadsheet Context initialized", module="spreadsheet-engine")
        else:
            variables["SPREADSHEET"] = {"enabled": False}
            code.debug("Spreadsheet Context disabled", module="spreadsheet-engine")

        # === INICIALIZAR CONTEXTOS (solo una vez) ===
        if AI_ENABLED:
            variables["AI"] = {
                "enabled": True,
                "functions": list(AI_FUNCTIONS.keys()),
                "version": "1.0.0"
            }
            code.info("AI Context initialized", module="ai-engine")
        else:
            variables["AI"] = {"enabled": False}
            code.debug("AI Context disabled", module="ai-engine")
            
        if COSMOS_ENABLED:
            variables["COSMOS"] = {
                "enabled": True,
                "functions": list(COSMOS_FUNCTIONS.keys()),
                "version": "1.0.0"
            }
            code.info("Cosmos Context initialized", module="cosmos-engine")
        else:
            variables["COSMOS"] = {"enabled": False}
            code.debug("Cosmos Context disabled", module="cosmos-engine")
            
        if CRYPTO_ENABLED:
            variables["CRYPTO"] = {
                "enabled": True,
                "functions": list(CRYPTO_FUNCTIONS.keys()),
                "version": CRYPTO_FUNCTIONS.get("__meta__", {}).get("version", "2.0.0"),
                "secure_level": CRYPTO_FUNCTIONS.get("__meta__", {}).get("secure_level", "high")
            }
            code.info("Crypto Context initialized", module="crypto-engine")
        else:
            variables["CRYPTO"] = {"enabled": False}
            code.debug("Crypto Context disabled", module="crypto-engine")
            
        if INSIGHT_ENABLED:
            variables["INSIGHT"] = {
                "enabled": True,
                "functions": list(INSIGHT_FUNCTIONS.keys()),
                "version": "1.0.0",
                "features": ["ocr", "table_detection", "signature_detection", "metadata_extraction"]
            }
            code.info("Insight Context initialized", module="insight-engine")
        else:
            variables["INSIGHT"] = {"enabled": False}
            code.debug("Insight Context disabled", module="insight-engine")
            
        if MATRIX_ENABLED:
            variables["MATRIX"] = {
                "enabled": True,
                "functions": list(MATRIX_FUNCTIONS.keys()),
                "version": "1.0.0",
                "features": ["smart_matrices", "neural_transforms", "quantum_ops", "3d_rotation"]
            }
            code.info("Matrix Context initialized", module="matrix-engine")
        else:
            variables["MATRIX"] = {"enabled": False}
            code.debug("Matrix Context disabled", module="matrix-engine")
        
        if QUANTUM_ENABLED:
            variables["QUANTUM"] = {
                "enabled": True,
                "functions": list(QUANTUM_FUNCTIONS.keys()),
                "version": "1.0.0",
                "features": ["qubits", "gates", "circuits", "entanglement", "noise_models", "measurements"]
            }
            code.info("Quantum Context initialized", module="quantum-engine")
        else:
            variables["QUANTUM"] = {"enabled": False}
            code.debug("Quantum Context disabled", module="quantum-engine")
            
        if TIMEWARP_ENABLED:
            variables["TIMEWARP"] = {
                "enabled": True,
                "functions": list(TIMEWARP_FUNCTIONS.keys()),
                "version": "1.0.0",
                "features": ["time_travel", "warp_clock", "timelines", "future_execution", "temporal_decorators", "performance_measurement"]
            }
            code.info("TimeWarp Context initialized", module="timewarp-engine")
        else:
            variables["TIMEWARP"] = {"enabled": False}
            code.debug("TimeWarp Context disabled", module="timewarp-engine")
            
        if VISION_ENABLED:
            variables["VISION"] = {
                "enabled": True,
                "functions": list(VISION_FUNCTIONS.keys()),
                "version": "1.0.0",
                "features": ["image_processing", "face_detection", "perceptual_hashing", "smart_cropping", "ocr", "seam_carving", "pipelines"]
            }
            code.info("Vision Context initialized", module="vision-engine")
        else:
            variables["VISION"] = {"enabled": False}
            code.debug("Vision Context disabled", module="vision-engine")

        _register_builtin_functions(functions)
        functions["_variables"] = variables

        # Registrar funciones FN antes de ejecutar el resto
        # code.progress("orion-core", "Registering user functions", 25)
        for node in ast:
            if node[0] == "FN":
                _, fn_name, params, body = node
                register_function(functions, fn_name, params, body)
        # Marcar como inicializado
        evaluate._initialized = True  

        # code.progress("orion-core", "Executing AST nodes", 50)
        # code.progress("orion-core", "Execution completed", 100)
    # 3. Si estamos en nivel superior
    if not inside_fn:
        if "main" in functions:
            code.trace("Executing main function", module="orion-runtime")
            main_functions = functions["main"]
                
            # functions["main"] es una lista de definiciones de función
            if isinstance(main_functions, list) and len(main_functions) > 0:
                main_def = main_functions[0]  # Tomar la primera definición
                    
                if isinstance(main_def, dict):
                    params = main_def.get("params", [])
                    body = main_def.get("body", [])
                else:
                    raise OrionRuntimeError(f"Formato de función inválido para 'main': {type(main_def)}")
            else:
                raise OrionRuntimeError("Función 'main' no encontrada o mal formateada")
                
            return evaluate(body, variables, functions, inside_fn=True)
          
    i = 0
    while i < len(ast):
        node = ast[i]
        tag = node[0]

        # --- Soporte para USE con o sin comillas ---
        if tag == "USE":
            _, module_path = node
            if module_path.startswith('"') and module_path.endswith('"'):
                base_name = module_path[1:-1]
            else:
                base_name = module_path

            code.info(f"Loading module: {base_name}", module="module-loader")

            # === IMPORTACIÓN ESPECIAL PARA MÓDULO AI ===
            if base_name == "ai" and AI_ENABLED:
                # Crear un objeto ai que contenga todas las funciones
                class AIModule:
                    def __init__(self, functions_dict):
                        for name, func in functions_dict.items():
                            setattr(self, name, func)
                
                # Crear instancia del módulo AI
                ai_module = AIModule(AI_FUNCTIONS)
                variables["ai"] = ai_module
                
                # También agregar las funciones individualmente por compatibilidad
                for ai_func_name, ai_func in AI_FUNCTIONS.items():
                    variables[ai_func_name] = ai_func
                variables["ai_enabled"] = True
                code.ok(f"Módulo AI importado con {len(AI_FUNCTIONS)} funciones", module="ai-loader")
                i += 1
                continue
            
            # === IMPORTACIÓN ESPECIAL PARA MÓDULO SPREADSHEET ===
            elif base_name == "spreadsheet" and SPREADSHEET_ENABLED:
                class SpreadsheetModule:
                    def __init__(self):
                        # Core functions
                        self.create = create
                        self.attach = attach  
                        self.register = spreadsheet_register
                        
                        # Bridge functions
                        self.sheet_register = LocalSheetBridge.register
                        self.sheet_attach = LocalSheetBridge.attach
                        
                        # Sync functions
                        self.push = OSync.push
                        self.pull = OSync.pull
                        self.status = OSync.status
                        self.list_synced = OSync.list_synced
                        
                        # Remote sync
                        self.link = LinkSheet.link
                        self.push_remote = LinkSheet.push
                        self.pull_remote = LinkSheet.pull
                        
                        # Protocol
                        self.osync = OSyncProtocol.execute

                spreadsheet_module = SpreadsheetModule()
                variables["spreadsheet"] = spreadsheet_module
                
                # También agregar funciones individualmente
                for func_name, func in SPREADSHEET_FUNCTIONS.items():
                    variables[func_name] = func
                    
                variables["spreadsheet_enabled"] = True
                code.ok(f"Módulo Spreadsheet importado con {len(SPREADSHEET_FUNCTIONS)} funciones", module="spreadsheet-loader")
                i += 1
                continue

            elif base_name == "osync" and SPREADSHEET_ENABLED:
                # Módulo específico para OSync Protocol
                class OSyncModule:
                    def __init__(self):
                        self.execute = OSyncProtocol.execute
                        self.push = OSync.push
                        self.pull = OSync.pull
                        self.status = OSync.status
                        self.list_synced = OSync.list_synced
                        
                osync_module = OSyncModule()
                variables["osync"] = osync_module
                variables["OSyncProtocol"] = OSyncProtocol
                variables["OSync"] = OSync
                
                code.ok("Módulo OSync importado", module="osync-loader")
                i += 1
                continue
            
            elif base_name == "cosmos" and COSMOS_ENABLED:
                class CosmosModule:
                    def __init__(self, functions_dict):
                        for name, func in functions_dict.items():
                            setattr(self, name, func)
                
                cosmos_module = CosmosModule(COSMOS_FUNCTIONS)
                variables["cosmos"] = cosmos_module
                
                for cosmos_func_name, cosmos_fun in COSMOS_FUNCTIONS.items():
                    variables[cosmos_func_name] = cosmos_fun
                variables["cosmos_enabled"] = True
                code.ok(f"Módulo Cosmos importado con {len(COSMOS_FUNCTIONS)} funciones", module="cosmos-loader")
                i += 1
                continue
            
            elif base_name == "crypto" and CRYPTO_ENABLED:
                class CryptoModule:
                    def __init__(self, functions_dict):
                        for name, func in functions_dict.items():
                            if name != "__meta__":
                                setattr(self, name, func)
                
                crypto_module = CryptoModule(CRYPTO_FUNCTIONS)
                variables["crypto"] = crypto_module
                
                for crypto_func_name, crypto_func in CRYPTO_FUNCTIONS.items():
                    if crypto_func_name != "__meta__":
                        variables[crypto_func_name] = crypto_func
                variables["crypto_enabled"] = True
                variables["crypto_meta"] = CRYPTO_FUNCTIONS.get("__meta__", {})
                code.ok(f"Módulo Crypto importado con {len([f for f in CRYPTO_FUNCTIONS if f != '__meta__'])} funciones", module="crypto-loader")
                i += 1
                continue
            
            elif base_name == "insight" and INSIGHT_ENABLED:
                # Crear un objeto insight que contenga todas las funciones
                class InsightModule:
                    def __init__(self, functions_dict):
                        for name, func in functions_dict.items():
                            if name == "insight" and isinstance(func, dict):
                                # Si insight contiene un diccionario de funciones
                                for sub_func_name, sub_func in func.items():
                                    setattr(self, sub_func_name, sub_func)
                            elif callable(func):
                                setattr(self, name, func)
                
                insight_module = InsightModule(INSIGHT_FUNCTIONS)
                variables["insight"] = insight_module
                
                # También agregar las funciones individualmente por compatibilidad
                for insight_func_name, insight_func in INSIGHT_FUNCTIONS.items():
                    if insight_func_name == "insight" and isinstance(insight_func, dict):
                        for sub_func_name, sub_func in insight_func.items():
                            variables[sub_func_name] = sub_func
                    elif callable(insight_func):
                        variables[insight_func_name] = insight_func
                variables["insight_enabled"] = True
                code.ok(f"Módulo Insight importado con {len(INSIGHT_FUNCTIONS)} funciones", module="insight-loader")
                i += 1
                continue
            
            elif base_name == "matrix" and MATRIX_ENABLED:
                # Crear un objeto matrix que contenga todas las funciones
                class MatrixModule:
                    def __init__(self, functions_dict):
                        for name, func in functions_dict.items():
                            setattr(self, name, func)
                
                matrix_module = MatrixModule(MATRIX_FUNCTIONS)
                variables["matrix"] = matrix_module
                
                # También agregar las funciones individualmente por compatibilidad
                for matrix_func_name, matrix_func in MATRIX_FUNCTIONS.items():
                    variables[matrix_func_name] = matrix_func
                variables["matrix_enabled"] = True
                code.ok(f"Módulo Matrix importado con {len(MATRIX_FUNCTIONS)} funciones", module="matrix-loader")
                i += 1
                continue
            
            elif base_name == "quantum" and QUANTUM_ENABLED:
                # Crear un objeto quantum que contenga todas las funciones
                class QuantumModule:
                    def __init__(self, functions_dict):
                        for name, func in functions_dict.items():
                            setattr(self, name, func)
                
                quantum_module = QuantumModule(QUANTUM_FUNCTIONS)
                variables["quantum"] = quantum_module
                
                # También agregar las funciones individualmente por compatibilidad
                for quantum_func_name, quantum_func in QUANTUM_FUNCTIONS.items():
                    variables[quantum_func_name] = quantum_func
                variables["quantum_enabled"] = True
                
                # Agregar puertas cuánticas como constantes
                from stdlib.quantum import H, X, Y, Z, I, S, T, CNOT
                variables["H"] = H
                variables["X"] = X
                variables["Y"] = Y
                variables["Z"] = Z
                variables["I"] = I
                variables["S"] = S
                variables["T"] = T
                variables["CNOT"] = CNOT
                code.ok(f"Módulo Quantum importado con {len(QUANTUM_FUNCTIONS)} funciones", module="quantum-loader")
                i += 1
                continue
            
            elif base_name == "timewarp" and TIMEWARP_ENABLED:
                # Crear un objeto timewarp que contenga todas las funciones
                class TimeWarpModule:
                    def __init__(self, functions_dict):
                        for name, func in functions_dict.items():
                            # Resolver conflictos de nombres
                            if name == "time_measure":
                                setattr(self, "time_measure", func)
                            else:
                                setattr(self, name, func)
                
                timewarp_module = TimeWarpModule(TIMEWARP_FUNCTIONS)
                variables["timewarp"] = timewarp_module
                
                # También agregar las funciones individualmente por compatibilidad
                for timewarp_func_name, timewarp_func in TIMEWARP_FUNCTIONS.items():
                    # Resolver conflictos de nombres
                    if timewarp_func_name == "time_measure":
                        variables["time_measure"] = timewarp_func
                    else:
                        variables[timewarp_func_name] = timewarp_func
                variables["timewarp_enabled"] = True
                code.ok(f"Módulo TimeWarp importado con {len(TIMEWARP_FUNCTIONS)} funciones", module="timewarp-loader")
                i += 1
                continue
            
            elif base_name == "vision" and VISION_ENABLED:
                # Crear un objeto vision que contenga todas las funciones
                class VisionModule:
                    def __init__(self, functions_dict):
                        for name, func in functions_dict.items():
                            if name == "vision" and isinstance(func, dict):
                                # Si vision contiene un diccionario de funciones
                                for sub_func_name, sub_func in func.items():
                                    setattr(self, sub_func_name, sub_func)
                            elif callable(func):
                                setattr(self, name, func)
                
                vision_module = VisionModule(VISION_FUNCTIONS)
                variables["vision"] = vision_module
                
                # También agregar las funciones individualmente por compatibilidad
                for vision_func_name, vision_func in VISION_FUNCTIONS.items():
                    if vision_func_name == "vision" and isinstance(vision_func, dict):
                        # Si vision contiene un diccionario de funciones
                        for sub_func_name, sub_func in vision_func.items():
                            variables[sub_func_name] = sub_func
                    elif callable(vision_func):
                        variables[vision_func_name] = vision_func
                
                variables["vision_enabled"] = True
                code.ok(f"Módulo Vision importado con {len(VISION_FUNCTIONS)} funciones", module="vision-loader")
                i += 1
                continue

            # --- Orion stdlib ---
            elif base_name == "json":
                variables["json"] = orion_json
                code.info(f"Módulo Orion stdlib '{base_name}' importado", module="stdlib-loader")
                i += 1
                continue

            #  Rutas posibles
            orion_file = os.path.join(os.getcwd(), base_name + ".orion")
            py_file = os.path.abspath(os.path.join(os.path.dirname(__file__), "..", "modules", base_name + ".py"))

            #  Si existe un módulo Orion local
            if os.path.exists(orion_file):
                from core.lexer import lex
                from core.parser import parse
                with open(orion_file, "r", encoding="utf-8") as f:
                    file_code = f.read() 
                imported_tokens = lex(file_code)
                imported_ast = parse(imported_tokens)
                evaluate(imported_ast, variables, functions)
                code.ok(f"Módulo Orion '{base_name}' ejecutado", module="orion-loader")

            # Si existe un módulo Python en /modules/
            elif os.path.exists(py_file):
                import sys
                sys.path.append(os.path.join(os.path.dirname(__file__), ".."))
                from modules import load_module
                mod_exports = load_module(variables, base_name)
                code.ok(f"Módulo Python '{base_name}' cargado con {len(mod_exports)} funciones", module="python-loader")

                for fname, fref in mod_exports.items():
                    if callable(fref):
                        register_native_function(functions, fname, fref)

            else:
                raise OrionRuntimeError(f"No se encontró el módulo: {orion_file} ni {py_file}")

            i += 1
            continue

        elif tag == "FN":
            # Ya registradas antes del bucle
            i += 1
            continue
        
        elif tag == "EXPR":
            # Evalúa la expresión y retorna el resultado
            if len(node) > 1:
                result = eval_expr(node[1], variables, functions)
            else:
                # Si no hay segundo elemento, probablemente es una expresión directa
                result = eval_expr(node, variables, functions)
            if inside_fn:
                return result
            # Si no está en función, mostrar el resultado (útil para REPL)
            return result
        
        # --- NUEVO: Manejar expresiones directas que no están envueltas en EXPR ---
        elif tag in ["BINARY_OP", "UNARY_OP", "CALL", "CALL_METHOD", "INDEX", "IDENT", "LIST", "DICT", "LAMBDA"]:
            # Es una expresión directa, evaluarla
            result = eval_expr(node, variables, functions)
            if inside_fn:
                return result
            # Si no está en función, mostrar el resultado (útil para REPL)
            return result
        
        # --- MANEJO DE CONTROL DE FLUJO ---
        elif tag == "IDENT":
            # Manejar instrucciones de control de flujo especiales
            _, name = node
            if name == "continue":
                raise ContinueException()
            elif name == "break":
                raise BreakException()
            else:
                # Variable normal
                if name in variables:
                    result = variables[name]
                else:
                    raise OrionNameError(f"Variable '{name}' no definida")
        
        elif tag == "ATTR_ASSIGN":
            # Asignación de atributo: ('ATTR_ASSIGN', objeto, atributo, valor)
            _, obj_expr, attr_name, value_expr = node
            obj = eval_expr(obj_expr, variables, functions)
            value = eval_expr(value_expr, variables, functions)
            if hasattr(obj, attr_name):
                setattr(obj, attr_name, value)
            elif isinstance(obj, dict):
                obj[attr_name] = value
            else:
                raise OrionRuntimeError(f"No se puede asignar atributo '{attr_name}' al tipo {type(obj).__name__}")

        elif tag == "ASSIGN":
            _, name, value = node
            val = eval_expr(value, variables, functions)
            # Envolver valores en tipos Orion si corresponde
            if isinstance(val, str) and not isinstance(val, OrionString):
                val = OrionString(val)
            elif isinstance(val, bool) and not isinstance(val, OrionBool):
                val = OrionBool(val)
            elif isinstance(val, int) and not isinstance(val, OrionNumber):
                val = OrionNumber(val)
            elif isinstance(val, float) and not isinstance(val, OrionNumber):
                val = OrionNumber(val)
            elif isinstance(val, list) and not isinstance(val, OrionList):
                val = OrionList(val)
            variables[name] = val
            
        elif tag == "INDEX_ASSIGN":
            # Asignación a índice: dict[key] = value
            _, dict_expr, key_expr, value_expr = node
            dict_obj = eval_expr(dict_expr, variables, functions)
            key = eval_expr(key_expr, variables, functions)
            value = eval_expr(value_expr, variables, functions)
            
            # Manejar diferentes tipos de contenedores
            if isinstance(dict_obj, dict):
                dict_obj[key] = value
            elif isinstance(dict_obj, list):
                try:
                    # Si el elemento actual es un string, reemplazarlo por una lista vacía
                    if isinstance(dict_obj[int(key)], str):
                        dict_obj[int(key)] = []
                    dict_obj[int(key)] = value
                except (ValueError, IndexError):
                    raise OrionRuntimeError(f"Índice inválido para lista: {key}")
            elif hasattr(dict_obj, 'items') and isinstance(dict_obj.items, list):
                try:
                    if isinstance(dict_obj.items[int(key)], str):
                        dict_obj.items[int(key)] = []
                    dict_obj.items[int(key)] = value
                except (ValueError, IndexError):
                    raise OrionRuntimeError(f"Índice inválido para OrionList: {key}")
            elif hasattr(dict_obj, 'value'):
                if isinstance(dict_obj.value, dict):
                    dict_obj.value[key] = value
                elif isinstance(dict_obj.value, list):
                    try:
                        if isinstance(dict_obj.value[int(key)], str):
                            dict_obj.value[int(key)] = []
                        dict_obj.value[int(key)] = value
                    except (ValueError, IndexError):
                        raise OrionRuntimeError(f"Índice inválido: {key}")
                else:
                    raise OrionRuntimeError(f"No se puede asignar índice al tipo {type(dict_obj.value)}")
            else:
                raise OrionRuntimeError(f"No se puede asignar índice al tipo {type(dict_obj)}")
            
        elif tag == "MULTI_ASSIGN":
            # MULTI_ASSIGN: ('MULTI_ASSIGN', ['var1', 'var2', ...], expression)
            _, var_names, value_expr = node
            values = eval_expr(value_expr, variables, functions)
            
            # Si el resultado es una tupla o lista, desempaquetarla
            if isinstance(values, (list, tuple)):
                if len(values) != len(var_names):
                    raise OrionRuntimeError(
                        f"No se puede desempaquetar {len(values)} valores en {len(var_names)} variables"
                    )
                for var_name, val in zip(var_names, values):
                    # Envolver valores en tipos Orion si corresponde
                    if isinstance(val, str) and not isinstance(val, OrionString):
                        val = OrionString(val)
                    elif isinstance(val, bool) and not isinstance(val, OrionBool):
                        val = OrionBool(val)
                    elif isinstance(val, int) and not isinstance(val, OrionNumber):
                        val = OrionNumber(val)
                    elif isinstance(val, float) and not isinstance(val, OrionNumber):
                        val = OrionNumber(val)
                    elif isinstance(val, list) and not isinstance(val, OrionList):
                        val = OrionList(val)
                    variables[var_name] = val
            else:
                raise OrionRuntimeError(
                    f"No se puede desempaquetar el valor {type(values).__name__} en múltiples variables"
                )
                
        

        elif tag == "DECLARE":
            _, type_name, var_name, expr_value = node
            if expr_value is not None:
                value = eval_expr(expr_value, variables, functions)
                # Envolver según el tipo declarado
                if type_name == "string" and not isinstance(value, OrionString):
                    value = OrionString(value)
                elif type_name == "int" and not isinstance(value, OrionNumber):
                    value = OrionNumber(value)
                elif type_name == "float" and not isinstance(value, OrionNumber):
                    value = OrionNumber(value)
                elif type_name == "bool" and not isinstance(value, OrionBool):
                    value = OrionBool(value)
                elif type_name == "list" and not isinstance(value, OrionList):
                    value = OrionList(value)
                elif type_name == "date" and not isinstance(value, OrionDate):
                    # Espera una tupla (año, mes, día) o un string "YYYY-MM-DD"
                    if isinstance(value, (tuple, list)) and len(value) == 3:
                        value = OrionDate(*value)
                    elif isinstance(value, str):
                        y, m, d = map(int, value.split("-"))
                        value = OrionDate(y, m, d)
                variables[var_name] = value
            else:
                if type_name == "int":
                    variables[var_name] = OrionNumber(0)
                elif type_name == "float":
                    variables[var_name] = OrionNumber(0.0)
                elif type_name == "bool":
                    variables[var_name] = OrionBool(False)
                elif type_name == "string":
                    variables[var_name] = OrionString("")
                elif type_name == "list":
                    variables[var_name] = OrionList([])
                elif type_name == "date":
                    variables[var_name] = OrionDate(1970, 1, 1)
                else:
                    variables[var_name] = None

        elif tag == "PRINT":
            _, value = node
            val = eval_expr(value, variables, functions)
            show.show(to_native(val))
            
        elif tag == "FOR_RANGE":
            # Consolidar ambos formatos en un solo bloque
            if len(node) == 4:
                _, var_name, range_args, body = node
                # range_args should be a list with arguments for range()
                rng = range(*[int(eval_expr(arg, variables, functions)) for arg in range_args])
            elif len(node) == 5:
                # Formato: ('FOR_RANGE', var_name, start, end, body)
                _, var_name, start_expr, end_expr, body = node
                start_val = eval_expr(start_expr, variables, functions)
                end_val = eval_expr(end_expr, variables, functions)
                
                if not isinstance(start_val, (int, float)):
                    raise OrionTypeError(f"El rango debe ser numérico, se recibió start={start_val}")
                if not isinstance(end_val, (int, float)):
                    raise OrionTypeError(f"El rango debe ser numérico, se recibió end={end_val}")
                
                rng = range(int(start_val), int(end_val) + 1)
            elif len(node) == 6:
                _, var_name, start, end, body, range_type = node
                start_val = eval_expr(start, variables, functions)
                end_val = eval_expr(end, variables, functions)
                
                if not isinstance(start_val, (int, float)):
                    raise OrionTypeError(f"El rango debe ser numérico, se recibió start={start_val}")
                if not isinstance(end_val, (int, float)):
                    raise OrionTypeError(f"El rango debe ser numérico, se recibió end={end_val}")
                
                if range_type == "RANGE":
                    rng = range(int(start_val), int(end_val) + 1)
                elif range_type == "RANGE_EX":
                    rng = range(int(start_val), int(end_val))
                else:
                    raise OrionRuntimeError(f"Tipo de rango no soportado: {range_type}")
            else:
                raise OrionRuntimeError(f"Formato de nodo FOR_RANGE no soportado: {node}")

            # Guardar valor previo de la variable del bucle
            prev_value = variables.get(var_name)
            
            for j in rng:
                variables[var_name] = j
                # Ejecutar el cuerpo del bucle con el flag inside_fn correcto
                result = evaluate(body, variables, functions, inside_fn=True)
                if inside_fn and result is not None:
                    return result
            
            # Limpiar scope
            if prev_value is not None:
                variables[var_name] = prev_value
            elif var_name in variables:
                del variables[var_name]

        elif tag == "FOR":
            _, var_name, start, end, body, range_type = node

            start_val = eval_expr(start, variables, functions)
            end_val = eval_expr(end, variables, functions)

            if not isinstance(start_val, (int, float)):
                raise OrionTypeError(f"El rango debe ser numérico, se recibió start={start_val}")
            if not isinstance(end_val, (int, float)):
                raise OrionTypeError(f"El rango debe ser numérico, se recibió end={end_val}")

            if range_type == "RANGE":
                rng = range(int(start_val), int(end_val) + 1)
            elif range_type == "RANGE_EX":
                rng = range(int(start_val), int(end_val))
            else:
                raise OrionRuntimeError(f"Tipo de rango no soportado: {range_type}")

            # Guardar valor previo (por si la variable ya existía antes)
            prev_value = variables.get(var_name)

            for j in rng:
                # Registrar variable del bucle en el mismo scope
                variables[var_name] = j

                # Ejecutar cuerpo del bucle
                result = evaluate(body, variables, functions, inside_fn=True)

                # Si el cuerpo devuelve algo (por ejemplo, un return), propágalo
                if inside_fn and result is not None:
                    return result

            # Limpieza del scope (restaurar o eliminar variable)
            if prev_value is not None:
                variables[var_name] = prev_value
            elif var_name in variables:
                del variables[var_name]

        elif tag == "FOR_IN":
            # FOR_IN puede tener diferentes formatos:
            # - ('FOR_IN', var_name, collection_expr, body) - una variable
            # - ('FOR_IN', [var1, var2], collection_expr, body) - múltiples variables

            if len(node) == 4:
                _, var_spec, collection_expr, body = node
            else:
                raise OrionRuntimeError(f"Formato de FOR_IN no soportado: {node}")

            # Evaluar la colección
            collection = eval_expr(collection_expr, variables, functions)

            # print(f"DEBUG: FOR_IN iterando sobre {type(collection)} - {collection}")

            if callable(collection):
                try:
                    collection = collection()
                    # print(f"DEBUG: Resultado de llamar función: {type(collection)} - {collection}")
                except Exception as e:
                    raise OrionRuntimeError(f"Error al llamar función en FOR_IN: {str(e)}")

            if hasattr(collection, "items"):
                collection = collection.items
            elif hasattr(collection, "value") and isinstance(collection.value, (list, dict)):
                collection = collection.value

            if not hasattr(collection, '__iter__') or isinstance(collection, (str, bytes)):
                if isinstance(collection, str):
                    pass
                else:
                    raise OrionRuntimeError(f"Objeto no es iterable en FOR_IN: {type(collection)}")

            if isinstance(var_spec, list):
                var_names = var_spec
                prev_values = {var_name: variables.get(var_name) for var_name in var_names}

                try:
                    if isinstance(collection, dict):
                        iterator = collection.items()
                    else:
                        iterator = collection

                    for item in iterator:
                        try:
                            if isinstance(collection, dict):
                                if len(var_names) == 2:
                                    variables[var_names[0]] = item[0]
                                    variables[var_names[1]] = item[1]
                                elif len(var_names) == 1:
                                    variables[var_names[0]] = item
                                else:
                                    if len(item) >= len(var_names):
                                        for i, var_name in enumerate(var_names):
                                            variables[var_name] = item[i] if i < len(item) else None
                                    else:
                                        for i, var_name in enumerate(var_names):
                                            variables[var_name] = item[i] if i < len(item) else None
                            else:
                                if len(var_names) == 1:
                                    variables[var_names[0]] = item
                                elif isinstance(item, (list, tuple)) and len(item) >= len(var_names):
                                    for i, var_name in enumerate(var_names):
                                        variables[var_name] = item[i] if i < len(item) else None
                                else:
                                    variables[var_names[0]] = item
                                    for i in range(1, len(var_names)):
                                        variables[var_names[i]] = None
                            
                            # Execute body and handle control flow exceptions
                            try:
                                result = evaluate(body, variables, functions, inside_fn=True)
                                if inside_fn and result is not None:
                                    return result
                            except ContinueException:
                                continue  # Continue to next iteration
                            except BreakException:
                                break     # Exit the loop completely
                                
                        except ContinueException:
                            continue
                        except BreakException:
                            break

                except BreakException:
                    pass  # Break caught at outer level
                except ContinueException:
                    pass  # Continue caught at outer level
                except TypeError as e:
                    raise OrionRuntimeError(f"Error de tipo al iterar en FOR_IN: {str(e)}")
                except Exception as e:
                    raise OrionRuntimeError(f"Error inesperado en FOR_IN: {str(e)}")

                # Restore previous variable values
                for var_name in var_names:
                    prev_value = prev_values[var_name]
                    if prev_value is not None:
                        variables[var_name] = prev_value
                    elif var_name in variables:
                        del variables[var_name]

            else:
                var_name = var_spec
                prev_value = variables.get(var_name)

                try:
                    if isinstance(collection, dict):
                        iterator = collection.items()
                    else:
                        iterator = collection

                    for item in iterator:
                        try:
                            if isinstance(collection, dict):
                                variables[var_name] = (item[0], item[1])
                            else:
                                variables[var_name] = item
                            
                            # Execute body and handle control flow exceptions
                            try:
                                result = evaluate(body, variables, functions, inside_fn=True)
                                if inside_fn and result is not None:
                                    return result
                            except ContinueException:
                                continue  # Continue to next iteration
                            except BreakException:
                                break     # Exit the loop completely
                                
                        except ContinueException:
                            continue
                        except BreakException:
                            break

                except BreakException:
                    pass  # Break caught at outer level
                except ContinueException:
                    pass  # Continue caught at outer level
                except TypeError as e:
                    raise OrionRuntimeError(f"Error de tipo al iterar en FOR_IN: {str(e)}")
                except Exception as e:
                    raise OrionRuntimeError(f"Error inesperado en FOR_IN: {str(e)}")

                # Restore previous variable value
                if prev_value is not None:
                    variables[var_name] = prev_value
                elif var_name in variables:
                    del variables[var_name]
        
        elif tag == "IF":
            _, condition, body_true, body_false = node
            if eval_expr(condition, variables, functions):
                result = evaluate(body_true, variables, functions, inside_fn=True)
            else:
                result = evaluate(body_false, variables, functions, inside_fn=True)
            if inside_fn and result is not None:
                return result
            
        elif tag == "IF_ELSIF":
            _, condition, body_true, elsif_parts, body_false = node
            
            # Evaluar condición principal
            if eval_expr(condition, variables, functions):
                result = evaluate(body_true, variables, functions, inside_fn=True)
                if inside_fn and result is not None:
                    return result
            else:
                # Evaluar condiciones elsif en orden
                executed = False
                for elsif_condition, elsif_body in elsif_parts:
                    if eval_expr(elsif_condition, variables, functions):
                        result = evaluate(elsif_body, variables, functions, inside_fn=True)
                        if inside_fn and result is not None:
                            return result
                        executed = True
                        break
                
                # Si ningún elsif se ejecutó, evaluar else final
                if not executed and body_false:
                    result = evaluate(body_false, variables, functions, inside_fn=True)
                    if inside_fn and result is not None:
                        return result

        elif tag == "CALL":
            eval_expr(node, variables, functions)
            
        elif tag == "CALL_METHOD":
            eval_expr(node, variables, functions)
            
        elif tag == "BINARY_OP":
            eval_expr(node, variables, functions)

        elif tag == "RETURN":
            _, value = node
            return eval_expr(value, variables, functions) if value is not None else None

        elif tag == "MATCH":
            result = eval_expr(node, variables, functions)
            if inside_fn and result is not None:
                return result
        
        elif tag == "ATTEMPT":
            try_body = node[1]
            handler = node[2] if len(node) > 2 else None
            
            try:
                # Ejecutar el bloque attempt
                result = None
                for stmt in try_body:
                    result = evaluate([stmt], variables, functions, inside_fn=True)
                return result if inside_fn else None
            except Exception as e:
                # Si hay un handler, ejecutarlo
                if handler and handler[0] == "HANDLE":
                    error_var = handler[1]
                    handle_body = handler[2]
                    
                    # Crear un nuevo scope para el handler con la variable de error
                    handler_vars = variables.copy()
                    # Convertir la excepción a un formato más amigable
                    error_msg = str(e) if hasattr(e, '__str__') else repr(e)
                    handler_vars[error_var] = error_msg
                    
                    result = None
                    for stmt in handle_body:
                        result = evaluate([stmt], handler_vars, functions, inside_fn=True)
                    return result if inside_fn else None
                else:
                    # Si no hay handler, re-lanzar la excepción
                    raise e

        elif tag == "HANDLE":
            # HANDLE nodes are processed as part of ATTEMPT
            raise OrionRuntimeError("HANDLE encontrado fuera de contexto ATTEMPT")

        else:
            raise OrionRuntimeError(f"Nodo desconocido en AST: {tag}")

        i += 1
        
    if "labels" in variables and "local" in variables and "clusters" in variables:
        labels = to_native(variables["labels"])
        local = to_native(variables["local"])
        clusters = variables["clusters"]
        for cluster_id in sorted(set(labels)):
            tasks_in = [t for j, t in enumerate(local) if labels[j] == cluster_id]
            if tasks_in:
                summary = think([t["title"] for t in tasks_in])["summary"]
            else:
                summary = "(sin contenido)"
            clusters[f"Cluster_{cluster_id}"] = summary
    
def to_native(val):
    from core.types import OrionList, OrionString, OrionNumber, OrionBool, OrionDate, OrionDict
    if isinstance(val, OrionList):
        return [to_native(v) for v in val.items]
    if isinstance(val, OrionDict):
        return {k: to_native(v) for k, v in val.value.items()}
    if isinstance(val, OrionString):
        return str(val.value)
    if isinstance(val, OrionNumber):
        return val.value
    if isinstance(val, OrionBool):
        return bool(val.value)
    if isinstance(val, OrionDate):
        return str(val)
    if isinstance(val, list):
        return [to_native(v) for v in val]
    if isinstance(val, dict):
        return {k: to_native(v) for k, v in val.items()}
    return val