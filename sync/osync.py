# ============================================================
# Orion Sync Engine (OSync)
# ============================================================
# Protocolo de sincronización híbrido local → remoto (futuro Orion Cloud)
# Maneja archivos sincronizados en storage/sync
# ============================================================

import os
import json
import time
import sys

# Agregar path para imports
sys.path.append(os.path.join(os.path.dirname(__file__), ".."))
from modules.sheets_localbridge import LocalSheetBridge

SYNC_PATH = "storage/sync"


class OSync:
    """Motor de sincronización Orion (Local + remoto futuro)."""

    @staticmethod
    def _ensure_sync_dir():
        os.makedirs(SYNC_PATH, exist_ok=True)

    # ------------------------------------------------------------
    # PUSH: exporta hoja local a JSON dentro de storage/sync/
    # ------------------------------------------------------------
    @staticmethod
    def push(sheet_id: str):
        try:
            OSync._ensure_sync_dir()
            bridge = LocalSheetBridge.attach(sheet_id)
            file_path = os.path.join(SYNC_PATH, f"{sheet_id}.json")

            # Exportar contenido usando el método dump
            data = OSync._dump_bridge_data(bridge)
            
            with open(file_path, "w", encoding="utf-8") as f:
                json.dump(
                    {
                        "sheet_id": sheet_id,
                        "timestamp": time.time(),
                        "content": data
                    },
                    f,
                    indent=2
                )
            print(f"[OSync] PUSH completado → {file_path}")
            return True
            
        except Exception as e:
            print(f"[OSync] Error en PUSH: {e}")
            return False

    # ------------------------------------------------------------
    # PULL: importa hoja desde JSON en storage/sync/
    # ------------------------------------------------------------
    @staticmethod
    def pull(sheet_id: str):
        try:
            OSync._ensure_sync_dir()
            file_path = os.path.join(SYNC_PATH, f"{sheet_id}.json")
            
            if not os.path.exists(file_path):
                print(f"[OSync] No hay datos sincronizados para '{sheet_id}'")
                return False

            # Cargar datos y restaurar hoja
            with open(file_path, "r", encoding="utf-8") as f:
                sync_data = json.load(f)

            bridge = LocalSheetBridge.attach(sheet_id)
            OSync._restore_bridge_data(bridge, sync_data["content"])
            bridge.save()

            print(f"[OSync] PULL restaurado desde {file_path}")
            return True
            
        except Exception as e:
            print(f"[OSync] Error en PULL: {e}")
            return False

    # ------------------------------------------------------------
    # Métodos auxiliares para dump/restore
    # ------------------------------------------------------------
    @staticmethod
    def _dump_bridge_data(bridge):
        """Extrae datos del bridge para sincronización."""
        try:
            if hasattr(bridge, 'ext'):
                if bridge.ext == ".xlsx":
                    # Para Excel, extraer todas las celdas con datos
                    data = {}
                    for row in bridge.ws.iter_rows():
                        for cell in row:
                            if cell.value is not None:
                                data[cell.coordinate] = cell.value
                    return {"type": "xlsx", "cells": data}
                    
                elif bridge.ext == ".csv":
                    # Para CSV, devolver las filas
                    return {"type": "csv", "rows": bridge.rows}
                    
            return {"type": "unknown", "data": None}
            
        except Exception as e:
            print(f"[OSync] Error en dump: {e}")
            return {"type": "error", "data": None}

    @staticmethod
    def _restore_bridge_data(bridge, data):
        """Restaura datos en el bridge desde sincronización."""
        try:
            if data["type"] == "xlsx" and bridge.ext == ".xlsx":
                # Restaurar celdas Excel
                for cell_coord, value in data["cells"].items():
                    bridge.ws[cell_coord] = value
                    
            elif data["type"] == "csv" and bridge.ext == ".csv":
                # Restaurar filas CSV
                bridge.rows = data["rows"]
                
        except Exception as e:
            print(f"[OSync] Error en restore: {e}")

    # ------------------------------------------------------------
    # Métodos de información
    # ------------------------------------------------------------
    @staticmethod
    def status(sheet_id: str):
        """Devuelve el estado de sincronización de una hoja."""
        OSync._ensure_sync_dir()
        file_path = os.path.join(SYNC_PATH, f"{sheet_id}.json")
        
        if os.path.exists(file_path):
            with open(file_path, "r", encoding="utf-8") as f:
                data = json.load(f)
            return {
                "exists": True,
                "last_sync": data.get("timestamp"),
                "sheet_id": data.get("sheet_id")
            }
        else:
            return {"exists": False}

    @staticmethod
    def list_synced():
        """Lista todas las hojas sincronizadas."""
        OSync._ensure_sync_dir()
        synced_sheets = []
        
        for file in os.listdir(SYNC_PATH):
            if file.endswith(".json"):
                sheet_id = file[:-5]  # Remover .json
                status = OSync.status(sheet_id)
                synced_sheets.append({
                    "sheet_id": sheet_id,
                    "last_sync": status.get("last_sync")
                })
                
        return synced_sheets
