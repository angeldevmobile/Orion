# ============================================================
# Orion Sync Engine (OSync)
# ============================================================
# Protocolo de sincronización híbrido local → remoto (futuro Orion Cloud)
# Maneja archivos sincronizados en storage/sync
# ============================================================

import os
import json
import time
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
        OSync._ensure_sync_dir()
        bridge = LocalSheetBridge.attach(sheet_id)
        file_path = os.path.join(SYNC_PATH, f"{sheet_id}.json")

        # Exportar contenido
        data = bridge.dump()
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

    # ------------------------------------------------------------
    # PULL: importa hoja desde JSON en storage/sync/
    # ------------------------------------------------------------
    @staticmethod
    def pull(sheet_id: str):
        OSync._ensure_sync_dir()
        file_path = os.path.join(SYNC_PATH, f"{sheet_id}.json")
        if not os.path.exists(file_path):
            print(f"[OSync] No hay datos sincronizados para '{sheet_id}'")
            return

        # Cargar datos y restaurar hoja
        with open(file_path, "r", encoding="utf-8") as f:
            data = json.load(f)

        bridge = LocalSheetBridge.attach(sheet_id)
        bridge.restore(data["content"])
        bridge.save()

        print(f"[OSync] PULL restaurado desde {file_path}")
