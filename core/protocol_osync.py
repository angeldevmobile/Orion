# ============================================================
# OSync Protocol Executor
# ============================================================

import sys
import os
sys.path.append(os.path.join(os.path.dirname(__file__), ".."))

from modules.sheets_localbridge import LocalSheetBridge
from sync.osync import OSync


class OSyncProtocol:
    """Interprete del protocolo Orion Sync (OSync)."""

    @staticmethod
    def execute(command: str):
        """
        Interpreta comandos OSync estilo:
        'PUT orion://sheet/ventas/A1 DATA: "Laptop"'
        'SYNC push ventas'
        'SYNC pull ventas'
        'SYNC push remote ventas'
        """
        parts = command.strip().split()
        if len(parts) < 2:
            raise ValueError("Comando OSync inválido")

        action, target = parts[0].upper(), parts[1]
        data = None
        
        # Extraer DATA si está presente
        if "DATA:" in command:
            data = command.split("DATA:", 1)[1].strip().strip('"')

        # Ejecutar acción
        if action == "PUT":
            # Parsear URL Orion
            if not target.startswith("orion://sheet/"):
                raise ValueError("PUT requiere URI tipo orion://sheet/...")

            # Extraer sheet_id y celda
            uri_parts = target.replace("orion://sheet/", "").split("/")
            sheet_id = uri_parts[0]
            cell = uri_parts[1] if len(uri_parts) > 1 else None
            
            if not cell or data is None:
                raise ValueError("PUT requiere celda y DATA")
                
            bridge = LocalSheetBridge.attach(sheet_id)
            bridge.write(cell, data)
            bridge.save()
            print(f"[OSyncProtocol] OK → {sheet_id}:{cell} = {data}")

        elif action == "SYNC":
            # Determinar el modo de sincronización
            if len(parts) < 3:
                raise ValueError("SYNC requiere modo (push/pull) y sheet_id")
                
            sync_mode = parts[1].lower()  # push, pull
            sheet_id = parts[2]
            is_remote = "remote" in parts
            
            if sync_mode == "push":
                if is_remote:
                    try:
                        from modules.linksheet import LinkSheet
                        LinkSheet.push(sheet_id)
                        print(f"[OSyncProtocol] Push remoto ejecutado para {sheet_id}")
                    except ImportError:
                        print(f"[OSyncProtocol] Error: LinkSheet no disponible")
                else:
                    OSync.push(sheet_id)
                    print(f"[OSyncProtocol] Push local ejecutado para {sheet_id}")
                    
            elif sync_mode == "pull":
                if is_remote:
                    try:
                        from modules.linksheet import LinkSheet
                        LinkSheet.pull(sheet_id)
                        print(f"[OSyncProtocol] Pull remoto ejecutado para {sheet_id}")
                    except ImportError:
                        print(f"[OSyncProtocol] Error: LinkSheet no disponible")
                else:
                    OSync.pull(sheet_id)
                    print(f"[OSyncProtocol] Pull local ejecutado para {sheet_id}")
            else:
                raise ValueError(f"Modo de sincronización no válido: {sync_mode}")

        else:
            raise ValueError(f"Acción OSync no reconocida: {action}")
