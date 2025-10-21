
# ============================================================
# OSync Protocol Executor
# ============================================================

from storage.localsheet import LocalSheetBridge
from sync.osync import OSync


class OSyncProtocol:
    """Interprete del protocolo Orion Sync (OSync)."""

    @staticmethod
    def execute(command: str):
        """
        Interpreta comandos OSync estilo:
        'PUT orion://sheet/ventas/A1 DATA: "Laptop"'
        """
        parts = command.strip().split()
        if len(parts) < 2:
            raise ValueError("Comando OSync inválido")

        action, target = parts[0].upper(), parts[1]
        data = None
        if "DATA:" in command:
            data = command.split("DATA:", 1)[1].strip().strip('"')

        # Parsear URL Orion
        if not target.startswith("orion://sheet/"):
            raise ValueError("Solo soporta URIs tipo orion://sheet/...")

        _, _, sheet_id, *path = target.split("/")
        cell = path[0] if path else None

        # Ejecutar acción
        if action == "PUT":
            if not cell or data is None:
                raise ValueError("PUT requiere celda y DATA")
            bridge = LocalSheetBridge.attach(sheet_id)
            bridge.write(cell, data)
            bridge.save()
            print(f"[OSyncProtocol] OK → {sheet_id}:{cell} = {data}")

        elif action == "SYNC":
            mode = "push" if "push" in command else "pull"
            if mode == "push":
                OSync.push(sheet_id)
            else:
                OSync.pull(sheet_id)
            print(f"[OSyncProtocol] Sincronización {mode} ejecutada para {sheet_id}")

        else:
            raise ValueError(f"Acción OSync no reconocida: {action}")
