import importlib.util
import sys
import os
import types

sys.path.append(os.path.join(os.path.dirname(__file__), ".."))
from core.control import eval_match
from core.functions import register_function, get_function, register_native_function
from core.types import (
    OrionString,
    OrionNumber,
    OrionBool,
    OrionDate,
    OrionList,
    null_safe,
)
from core.errors import (
    OrionRuntimeError,
    OrionTypeError,
    OrionNameError,
    OrionFunctionError,
)

from modules import json as orion_json
from lib import collections
from modules import code, show
from lib import io
from lib import math as orion_math
from lib import strings

# === INTEGRACIÓN DEL MÓDULO AI ===
try:
    from stdlib.ai import orion_export, think, quantum_embed, recall
    AI_ENABLED = True
    AI_FUNCTIONS = orion_export()
    print("[DEBUG] Módulo AI Orion cargado exitosamente")
except ImportError as e:
    AI_ENABLED = False
    AI_FUNCTIONS = {}
    print(f"[DEBUG] Módulo AI no disponible: {e}")
    
# ==========================================
try: 
    from stdlib.cosmos import orion_export, cosmos, Body, Universe
    COSMOS_ENABLED = True
    COSMOS_FUNCTIONS = orion_export()
    print("[DEBUG] Módulo Cosmos Orion cargado exitosamente")
except ImportError as e:
    COSMOS_ENABLED = False
    COSMOS_FUNCTIONS = {}
    print(f"[DEBUG] Módulo Cosmos no disponible: {e}")
    
# =========================================
try:
    from stdlib.crypto import orion_export as crypto_export, crypto, hash, encrypt, decrypt, sign, verify
    CRYPTO_ENABLED = True
    CRYPTO_FUNCTIONS = crypto_export()
    print("[DEBUG] Módulo Crypto Orion cargado exitosamente")
except ImportError as e:
    CRYPTO_ENABLED = False
    CRYPTO_FUNCTIONS = {}
    print(f"[DEBUG] Módulo Crypto no disponible: {e}")
    
# ============================================================
try:
    from stdlib.insight import orion_export as insight_export, extract_text_blocks, extract_tables, extract_metadata, extract_signatures, summarize
    INSIGHT_ENABLED = True
    INSIGHT_FUNCTIONS = insight_export()
    print("[DEBUG] Módulo Insight Orion cargado exitosamente")
except ImportError as e:
    INSIGHT_ENABLED = False
    INSIGHT_FUNCTIONS = {}
    print(f"[DEBUG] Módulo Insight no disponible: {e}")
    
# ============================================================
try:
    from stdlib.matrix import orion_export as matrix_export, matrix, SmartMatrix, add, mul, transpose, det, inverse, neuralify, morph
    MATRIX_ENABLED = True
    MATRIX_FUNCTIONS = matrix_export()
    print("[DEBUG] Módulo Matrix Orion cargado exitosamente")
except ImportError as e:
    MATRIX_ENABLED = False
    MATRIX_FUNCTIONS = {}
    print(f"[DEBUG] Módulo Matrix no disponible: {e}")   
    
# ============================================================
try:
    from stdlib.quantum import orion_export as quantum_export, quantum, qubit, bell_pair, measure, apply_gate, tensor, fidelity
    QUANTUM_ENABLED = True
    QUANTUM_FUNCTIONS = quantum_export()
    print("[DEBUG] Módulo Quantum Orion cargado exitosamente")
except ImportError as e:
    QUANTUM_ENABLED = False
    QUANTUM_FUNCTIONS = {}
    print(f"[DEBUG] Módulo Quantum no disponible: {e}")

try:
    from stdlib.timewarp import orion_export as timewarp_export, timewarp, WarpClock, TimeLine, future, warp_speed, wait, measureMtime
    TIMEWARP_ENABLED = True
    TIMEWARP_FUNCTIONS = timewarp_export()
    print("[DEBUG] Módulo TimeWarp Orion cargado exitosamente")
except ImportError as e:
    TIMEWARP_ENABLED = False
    TIMEWARP_FUNCTIONS = {}
    print(f"[DEBUG] Módulo TimeWarp no disponible: {e}")

try:
    from stdlib.vision import orion_export as vision_export, load, save, resize, smart_crop, dhash, detect_faces, blur_faces, ImagePipeline
    VISION_ENABLED = True
    VISION_FUNCTIONS = vision_export()
    print("[DEBUG] Módulo Vision Orion cargado exitosamente")
except ImportError as e:
    VISION_ENABLED = False
    VISION_FUNCTIONS = {}
    print(f"[DEBUG] Módulo Vision no disponible: {e}")

NATIVE_FUNCTIONS = {
    "trace_start": code.trace_start,
    "trace_end": code.trace_end,
    "progress": code.progress,
    "divider": code.divider,
    "frame": code.frame,
    "show": show.show,
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
    
    # Registrar funciones nativas de Python necesarias
    register_native_function(functions, "len", len)
    
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
        
        print(f"[DEBUG] {len(AI_FUNCTIONS)} funciones AI registradas como built-ins")
        
    if COSMOS_ENABLED:
        for cosmos_func_name, cosmos_fun in COSMOS_FUNCTIONS.items():
            if callable(cosmos_fun):
                register_native_function(functions, cosmos_func_name, cosmos_fun)
        register_native_function(functions, "cosmos_create", lambda *args, **kwargs: cosmos("create", *args, **kwargs))
        register_native_function(functions, "cosmos_run", lambda *args, **kwargs: cosmos("run", *args, **kwargs))
        register_native_function(functions, "cosmos_dust", lambda *args, **kwargs: cosmos("dust", *args, **kwargs))
        print(f"[DEBUG] {len(COSMOS_FUNCTIONS)} funciones Cosmos registradas como built-ins")

    if CRYPTO_ENABLED:
        for crypto_func_name, crypto_func in CRYPTO_FUNCTIONS.items():
            if callable(crypto_func) and crypto_func_name != "__meta__":
                register_native_function(functions, crypto_func_name, crypto_func)
        register_native_function(functions, "crypto_hash", lambda *args, **kwargs: crypto("hash", *args, **kwargs))
        register_native_function(functions, "crypto_encrypt", lambda *args, **kwargs: crypto("encrypt", *args, **kwargs))
        register_native_function(functions, "crypto_decrypt", lambda *args, **kwargs: crypto("decrypt", *args, **kwargs))
        print(f"[DEBUG] {len([f for f in CRYPTO_FUNCTIONS if callable(CRYPTO_FUNCTIONS[f])])} funciones Crypto registradas como built-ins")
    
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
        
        print(f"[DEBUG] {len([f for f in INSIGHT_FUNCTIONS if callable(INSIGHT_FUNCTIONS.get(f, {}).get if isinstance(INSIGHT_FUNCTIONS.get(f), dict) else INSIGHT_FUNCTIONS.get(f))])} funciones Insight registradas como built-ins")

    if MATRIX_ENABLED:
        for matrix_func_name, matrix_func in MATRIX_FUNCTIONS.items():
            if callable(matrix_func):
                register_native_function(functions, matrix_func_name, matrix_func)
        register_native_function(functions, "matrix_add", lambda *args: matrix("add", *args))
        register_native_function(functions, "matrix_mul", lambda *args: matrix("mul", *args))
        register_native_function(functions, "matrix_det", lambda *args: matrix("det", *args))
        register_native_function(functions, "matrix_inv", lambda *args: matrix("inverse", *args))
        print(f"[DEBUG] {len(MATRIX_FUNCTIONS)} funciones Matrix registradas como built-ins")

    if QUANTUM_ENABLED:
        for quantum_func_name, quantum_func in QUANTUM_FUNCTIONS.items():
            if callable(quantum_func):
                register_native_function(functions, quantum_func_name, quantum_func)
        register_native_function(functions, "quantum_qubit", lambda *args: quantum("qubit", *args))
        register_native_function(functions, "quantum_bell", lambda *args: quantum("bell", *args))
        register_native_function(functions, "quantum_measure", lambda *args, **kwargs: quantum("measure", *args, **kwargs))
        register_native_function(functions, "quantum_circuit", lambda *args: quantum("apply_circuit", *args))
        
        print(f"[DEBUG] {len(QUANTUM_FUNCTIONS)} funciones Quantum registradas como built-ins")

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
        
        print(f"[DEBUG] {len(TIMEWARP_FUNCTIONS)} funciones TimeWarp registradas como built-ins")

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
        
        print(f"[DEBUG] {len(VISION_FUNCTIONS)} funciones Vision registradas como built-ins")


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
            val = variables[name]
            if hasattr(val, "value"):
                val = val.value

            # Conversión automática de strings numéricos
            if isinstance(val, str):
                if val.isdigit():
                    return int(val)
                try:
                    return float(val)
                except ValueError:
                    pass
            return val
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

            # Si es diccionario
            if isinstance(list_val, dict):
                return list_val.get(index_val, None)

            # No indexable
            raise OrionRuntimeError(f"No se puede indexar el tipo {type(list_val).__name__}")

        # --- LIST ---
        elif tag == "LIST":
            _, elements = expr
            return [eval_expr(e, variables, functions) for e in elements]
        
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
                val = variables[name]
                if hasattr(val, "value"):
                    return val.value
                return val
            else:
                raise OrionRuntimeError(f"Variable '{name}' no definida")

        # --- TYPE ---
        elif tag == "TYPE":
            _, type_name = expr
            return type_name  # Retorna el nombre del tipo como string

        # --- BINARY_OP ---
        elif tag == "BINARY_OP":
            _, op, left, right = expr
            left_val = eval_expr(left, variables, functions)
            right_val = eval_expr(right, variables, functions)

            if hasattr(left_val, "value"):
                left_val = left_val.value
            if hasattr(right_val, "value"):
                right_val = right_val.value

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

            left_val = try_cast_numeric(left_val)
            right_val = try_cast_numeric(right_val)

            # --- OPERADORES BINARIOS ---
            if op == "+":
                if isinstance(left_val, str) or isinstance(right_val, str):
                    return str(left_val) + str(right_val)
                return left_val + right_val
            elif op == "-":
                return left_val - right_val
            elif op == "*":
                return left_val * right_val
            elif op == "/":
                return left_val / right_val
            elif op in [">", "<", ">=", "<=", "==", "!="]:
                # Intentar normalizar tipos antes de comparar
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

                # Comparaciones de texto
                if isinstance(left_val, str) and isinstance(right_val, str):
                    if op == ">": return left_val > right_val
                    if op == "<": return left_val < right_val
                    if op == ">=": return left_val >= right_val
                    if op == "<=": return left_val <= right_val
                    if op == "==": return left_val == right_val
                    if op == "!=": return left_val != right_val

                # Si uno es string numérico y otro número
                if isinstance(left_val, str) and left_val.replace('.', '', 1).isdigit():
                    left_val = float(left_val) if '.' in left_val else int(left_val)
                    return eval_expr(("BINARY_OP", op, left_val, right_val), variables, functions)
                if isinstance(right_val, str) and right_val.replace('.', '', 1).isdigit():
                    right_val = float(right_val) if '.' in right_val else int(right_val)
                    return eval_expr(("BINARY_OP", op, left_val, right_val), variables, functions)

                raise OrionRuntimeError(
                    f"No se puede comparar {type(left_val).__name__} con {type(right_val).__name__}"
                )

            elif op == "&&":
                return bool(left_val) and bool(right_val)
            elif op == "||":
                return bool(left_val) or bool(right_val)
            else:
                raise OrionRuntimeError(f"Operador binario desconocido: {op}")

        # --- UNARY_OP ---
        elif tag == "UNARY_OP":
            _, op, operand = expr
            if op == "!":
                return not eval_expr(operand, variables, functions)
            else:
                raise OrionRuntimeError(f"Operador unario desconocido: {op}")

        # --- CALL ---
        elif tag == "CALL":
            _, fn_name, args = expr

            fn_def = get_function(functions, fn_name)
            if fn_def is None and fn_name in NATIVE_FUNCTIONS:
                fn_def = {
                    "type": "NATIVE_FN",
                    "impl": NATIVE_FUNCTIONS[fn_name]
                }
            if not fn_def:
                raise OrionFunctionError(f"Función no definida: {fn_name}")

            pos_args, kw_args = eval_call_args(args, variables, functions)

            # Función nativa
            if fn_def["type"] == "NATIVE_FN":
                # === MANEJO ESPECIAL PARA FUNCIONES AI ===
                if AI_ENABLED and (fn_name in AI_FUNCTIONS or fn_name.startswith('ai_')):
                    try:
                        result = fn_def["impl"](*pos_args, **kw_args)
                        # Registrar la operación AI en memoria si es think/embed/recall
                        if fn_name in ["think", "ai_think", "embed", "ai_embed"]:
                            print(f"[DEBUG AI] Ejecutado {fn_name} con {len(pos_args)} argumentos")
                        return result
                    except Exception as e:
                        raise OrionRuntimeError(f"Error en función AI '{fn_name}': {str(e)}")
                
                # === MANEJO ESPECIAL PARA FUNCIONES COSMOS ===
                elif COSMOS_ENABLED and (fn_name in COSMOS_FUNCTIONS or fn_name.startswith('cosmos_')):
                    try:
                        result = fn_def["impl"](*pos_args, **kw_args)
                        if fn_name in ["cosmos", "cosmos_create", "cosmos_run"]:
                            print(f"[DEBUG COSMOS] Ejecutado {fn_name} con {len(pos_args)} argumentos")
                        return result
                    except Exception as e:
                        raise OrionRuntimeError(f"Error en función Cosmos '{fn_name}': {str(e)}")
                
                # === MANEJO ESPECIAL PARA FUNCIONES CRYPTO ===
                elif CRYPTO_ENABLED and (fn_name in CRYPTO_FUNCTIONS or fn_name.startswith('crypto_')):
                    try:
                        result = fn_def["impl"](*pos_args, **kw_args)
                        if fn_name in ["crypto", "hash", "encrypt", "decrypt", "sign", "verify"]:
                            print(f"[DEBUG CRYPTO] Ejecutado {fn_name} con {len(pos_args)} argumentos")
                        return result
                    except Exception as e:
                        raise OrionRuntimeError(f"Error en función Crypto '{fn_name}': {str(e)}")
                
                # === MANEJO ESPECIAL PARA FUNCIONES INSIGHT ===
                elif INSIGHT_ENABLED and (fn_name in INSIGHT_FUNCTIONS or fn_name.startswith('insight_')):
                    try:
                        result = fn_def["impl"](*pos_args, **kw_args)
                        if fn_name in ["extract_text_blocks", "extract_tables", "extract_metadata", "extract_signatures", "summarize"]:
                            print(f"[DEBUG INSIGHT] Ejecutado {fn_name} con {len(pos_args)} argumentos")
                        return result
                    except Exception as e:
                        raise OrionRuntimeError(f"Error en función Insight '{fn_name}': {str(e)}")
                
                # === MANEJO ESPECIAL PARA FUNCIONES MATRIX ===
                elif MATRIX_ENABLED and (fn_name in MATRIX_FUNCTIONS or fn_name.startswith('matrix_')):
                    try:
                        result = fn_def["impl"](*pos_args, **kw_args)
                        if fn_name in ["matrix_add", "matrix_mul", "matrix_det", "matrix_inv"]:
                            print(f"[DEBUG MATRIX] Ejecutado {fn_name} con {len(pos_args)} argumentos")
                        return result
                    except Exception as e:
                        raise OrionRuntimeError(f"Error en función Matrix '{fn_name}': {str(e)}")

                # === MANEJO ESPECIAL PARA FUNCIONES QUANTUM ===
                if QUANTUM_ENABLED and (fn_name in QUANTUM_FUNCTIONS or fn_name.startswith('quantum_') or
                                        fn_name in ["qubit", "bell_pair", "measure", "apply_gate", "tensor", "fidelity"]):
                    try:
                        result = fn_def["impl"](*pos_args, **kw_args)
                        if fn_name in ["quantum", "qubit", "bell_pair", "measure", "apply_circuit"]:
                            print(f"[DEBUG QUANTUM] Ejecutado {fn_name} - operación cuántica completada")
                        return result
                    except Exception as e:
                        raise OrionRuntimeError(f"Error en función Quantum '{fn_name}': {str(e)}")
                
                # === MANEJO ESPECIAL PARA FUNCIONES TIMEWARP ===
                if TIMEWARP_ENABLED and (fn_name in TIMEWARP_FUNCTIONS or fn_name.startswith('timewarp_') or 
                                       fn_name in ["WarpClock", "TimeLine", "future", "warp_speed", "wait", "time_measure"]):
                    try:
                        result = fn_def["impl"](*pos_args, **kw_args)
                        if fn_name in ["timewarp", "WarpClock", "future", "wait", "time_measure"]:
                            print(f"[DEBUG TIMEWARP] Ejecutado {fn_name} - operación temporal completada")
                        return result
                    except Exception as e:
                        raise OrionRuntimeError(f"Error en función TimeWarp '{fn_name}': {str(e)}")

                # === MANEJO ESPECIAL PARA FUNCIONES VISION ===
                if VISION_ENABLED and (fn_name in VISION_FUNCTIONS or fn_name.startswith('vision_') or 
                                     fn_name in ["load", "save", "resize", "smart_crop", "dhash", "detect_faces", "blur_faces", "ImagePipeline"]):
                    try:
                        result = fn_def["impl"](*pos_args, **kw_args)
                        if fn_name in ["load", "save", "resize", "smart_crop", "detect_faces", "blur_faces"]:
                            print(f"[DEBUG VISION] Ejecutado {fn_name} - procesamiento de imagen completado")
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
            _, method_name, obj_expr, args = expr
            obj_val = eval_expr(obj_expr, variables, functions)
            pos_args, kw_args = eval_call_args(args, variables, functions)
            if hasattr(orion_math, method_name):
                fn = getattr(orion_math, method_name)
                return fn(obj_val, *pos_args, **kw_args)
            method = getattr(obj_val, method_name, None)
            if callable(method):
                return method(*pos_args, **kw_args)
            raise OrionFunctionError(
                f"Método '{method_name}' no definido en lib.math ni en el objeto."
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

    # === Inicializar valores nativos de Orion ===
    if "null" not in variables:
        variables["null"] = None
    if "yes" not in variables:
        variables["yes"] = OrionBool(True)
    if "no" not in variables:
        variables["no"] = OrionBool(False)

    # === INICIALIZAR CONTEXTO AI ===
    if AI_ENABLED:
        variables["AI"] = {
            "enabled": True,
            "functions": list(AI_FUNCTIONS.keys()),
            "version": "1.0.0"
        }
    else:
        variables["AI"] = {"enabled": False}
        
    if COSMOS_ENABLED:
        variables["COSMOS"] = {
            "enabled": True,
            "functions": list(COSMOS_FUNCTIONS.keys()),
            "version": "1.0.0"
        }
    else:
        variables["COSMOS"] = {"enabled": False}
        
    if CRYPTO_ENABLED:
        variables["CRYPTO"] = {
            "enabled": True,
            "functions": list(CRYPTO_FUNCTIONS.keys()),
            "version": CRYPTO_FUNCTIONS.get("__meta__", {}).get("version", "2.0.0"),
            "secure_level": CRYPTO_FUNCTIONS.get("__meta__", {}).get("secure_level", "high")
        }
    else:
        variables["CRYPTO"] = {"enabled": False}
        
    if INSIGHT_ENABLED:
        variables["INSIGHT"] = {
            "enabled": True,
            "functions": list(INSIGHT_FUNCTIONS.keys()),
            "version": "1.0.0",
            "features": ["ocr", "table_detection", "signature_detection", "metadata_extraction"]
        }
    else:
        variables["INSIGHT"] = {"enabled": False}
        
    if MATRIX_ENABLED:
        variables["MATRIX"] = {
            "enabled": True,
            "functions": list(MATRIX_FUNCTIONS.keys()),
            "version": "1.0.0",
            "features": ["smart_matrices", "neural_transforms", "quantum_ops", "3d_rotation"]
        }
    else:
        variables["MATRIX"] = {"enabled": False}
    
    if QUANTUM_ENABLED:
        variables["QUANTUM"] = {
            "enabled": True,
            "functions": list(QUANTUM_FUNCTIONS.keys()),
            "version": "1.0.0",
            "features": ["qubits", "gates", "circuits", "entanglement", "noise_models", "measurements"]
        }
    else:
        variables["QUANTUM"] = {"enabled": False}
        
    if TIMEWARP_ENABLED:
        variables["TIMEWARP"] = {
            "enabled": True,
            "functions": list(TIMEWARP_FUNCTIONS.keys()),
            "version": "1.0.0",
            "features": ["time_travel", "warp_clock", "timelines", "future_execution", "temporal_decorators", "performance_measurement"]
        }
    else:
        variables["TIMEWARP"] = {"enabled": False}
        
    if VISION_ENABLED:
        variables["VISION"] = {
            "enabled": True,
            "functions": list(VISION_FUNCTIONS.keys()),
            "version": "1.0.0",
            "features": ["image_processing", "face_detection", "perceptual_hashing", "smart_cropping", "ocr", "seam_carving", "pipelines"]
        }
    else:
        variables["VISION"] = {"enabled": False}

    _register_builtin_functions(functions)
    functions["_variables"] = variables

    # Registrar funciones FN antes de ejecutar el resto
    for node in ast:
        if node[0] == "FN":
            _, fn_name, params, body = node
            register_function(functions, fn_name, params, body)

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

            print(f"[DEBUG USE] Cargando módulo: {base_name}")

            # === IMPORTACIÓN ESPECIAL PARA MÓDULO AI ===
            if base_name == "ai" and AI_ENABLED:
                # Agregar todas las funciones AI al entorno de variables
                for ai_func_name, ai_func in AI_FUNCTIONS.items():
                    variables[ai_func_name] = ai_func
                variables["ai_enabled"] = True
                print(f"[DEBUG] Módulo AI importado con {len(AI_FUNCTIONS)} funciones")
                i += 1
                continue
            elif base_name == "cosmos" and COSMOS_ENABLED:
                for cosmos_func_name, cosmos_fun in COSMOS_FUNCTIONS.items():
                    variables[cosmos_func_name] = cosmos_fun
                variables["cosmos_enabled"] = True
                print(f"[DEBUG] Módulo Cosmos importado con {len(COSMOS_FUNCTIONS)} funciones")
                i += 1
                continue
            
            elif base_name == "crypto" and CRYPTO_ENABLED:
                for crypto_func_name, crypto_func in CRYPTO_FUNCTIONS.items():
                    if crypto_func_name != "__meta__":
                        variables[crypto_func_name] = crypto_func
                variables["crypto_enabled"] = True
                variables["crypto_meta"] = CRYPTO_FUNCTIONS.get("__meta__", {})
                print(f"[DEBUG] Módulo Crypto importado con {len([f for f in CRYPTO_FUNCTIONS if f != '__meta__'])} funciones")
                i += 1
                continue
            
            elif base_name == "insight" and INSIGHT_ENABLED:
                for insight_func_name, insight_func in INSIGHT_FUNCTIONS.items():
                    if insight_func_name == "insight" and isinstance(insight_func, dict):
                        for sub_func_name, sub_func in insight_func.items():
                            variables[sub_func_name] = sub_func
                    elif callable(insight_func):
                        variables[insight_func_name] = insight_func
                variables["insight_enabled"] = True
                print(f"[DEBUG] Módulo Insight importado con {len(INSIGHT_FUNCTIONS)} funciones")
                i += 1
                continue
            
            elif base_name == "matrix" and MATRIX_ENABLED:
                for matrix_func_name, matrix_func in MATRIX_FUNCTIONS.items():
                    variables[matrix_func_name] = matrix_func
                variables["matrix_enabled"] = True
                print(f"[DEBUG] Módulo Matrix importado con {len(MATRIX_FUNCTIONS)} funciones")
                i += 1
                continue
            
            elif base_name == "quantum" and QUANTUM_ENABLED:
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
                print(f"[DEBUG] Módulo Quantum importado con {len(QUANTUM_FUNCTIONS)} funciones")
                i += 1
                continue
            
            elif base_name == "timewarp" and TIMEWARP_ENABLED:
                for timewarp_func_name, timewarp_func in TIMEWARP_FUNCTIONS.items():
                    # Resolver conflictos de nombres
                    if timewarp_func_name == "time_measure":
                        variables["time_measure"] = timewarp_func
                    else:
                        variables[timewarp_func_name] = timewarp_func
                variables["timewarp_enabled"] = True
                print(f"[DEBUG] Módulo TimeWarp importado con {len(TIMEWARP_FUNCTIONS)} funciones")
                i += 1
                continue
            
            elif base_name == "vision" and VISION_ENABLED:
                for vision_func_name, vision_func in VISION_FUNCTIONS.items():
                    if vision_func_name == "vision" and isinstance(vision_func, dict):
                        # Si vision contiene un diccionario de funciones
                        for sub_func_name, sub_func in vision_func.items():
                            variables[sub_func_name] = sub_func
                    elif callable(vision_func):
                        variables[vision_func_name] = vision_func
                
                variables["vision_enabled"] = True
                print(f"[DEBUG] Módulo Vision importado con {len(VISION_FUNCTIONS)} funciones")
                i += 1
                continue

            # --- Orion stdlib ---
            elif base_name == "json":
                variables["json"] = orion_json
                print(f"[DEBUG] Módulo Orion stdlib '{base_name}' importado")
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
                    code = f.read()
                imported_tokens = lex(code)
                imported_ast = parse(imported_tokens)
                evaluate(imported_ast, variables, functions)
                print(f"[DEBUG] Módulo Orion '{base_name}' ejecutado")

            # Si existe un módulo Python en /modules/
            elif os.path.exists(py_file):
                import sys
                sys.path.append(os.path.join(os.path.dirname(__file__), ".."))
                from modules import load_module
                mod_exports = load_module(variables, base_name)
                print(f"[DEBUG] Módulo Python '{base_name}' cargado con {len(mod_exports)} funciones")

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
            show.show(val)

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
            # FOR_IN: ('FOR_IN', var_name, collection_expr, body)
            var_name, collection_expr, body = node[1], node[2], node[3]
            collection = eval_expr(collection_expr, variables, functions)
            # Convierte OrionList a lista nativa si es necesario
            if hasattr(collection, "items"):
                collection = collection.items
            elif hasattr(collection, "value") and isinstance(collection.value, list):
                collection = collection.value
            prev_value = variables.get(var_name)
            for item in collection:
                variables[var_name] = item
                result = evaluate(body, variables, functions, inside_fn=True)
                if inside_fn and result is not None:
                    return result
            # Limpieza del scope
            if prev_value is not None:
                variables[var_name] = prev_value
            elif var_name in variables:
                del variables[var_name]
        
        elif tag == "FOR_RANGE":
            # FOR_RANGE: ('FOR_RANGE', var_name, start, end, body, range_type)
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

        elif tag == "IF":
            _, condition, body_true, body_false = node
            if eval_expr(condition, variables, functions):
                result = evaluate(body_true, variables, functions, inside_fn=True)
            else:
                result = evaluate(body_false, variables, functions, inside_fn=True)
            if inside_fn and result is not None:
                return result

        elif tag == "CALL":
            eval_expr(node, variables, functions)

        elif tag == "RETURN":
            _, value = node
            return eval_expr(value, variables, functions) if value is not None else None

        elif tag == "MATCH":
            result = eval_expr(node, variables, functions)
            if inside_fn and result is not None:
                return result

        else:
            raise OrionRuntimeError(f"Nodo desconocido en AST: {tag}")

        i += 1

    # 3. Si estamos en nivel superior
    if not inside_fn:
        if "main" in functions:
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
        return None
    
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