"""
Orion LinkSheet — Puente seguro entre hojas locales y remotas (Google Sheets)
sin depender de APIs oficiales ni puertos abiertos.
"""

import json
import urllib.request
from modules.sheets_localbridge import LocalSheetBridge

LINKS = {}

class LinkSheet:
    @staticmethod
    def link(sheet_id, url, token):
        """Asocia una hoja local a un webhook remoto (Apps Script)."""
        LINKS[sheet_id] = {"url": url, "token": token}
        print(f"[LinkSheet] '{sheet_id}' vinculado a {url}")
        return True

    @staticmethod
    def push(sheet_id):
        """Envía el contenido local hacia Google Sheets vía webhook."""
        if sheet_id not in LINKS:
            print(f"[LinkSheet] No hay enlace definido para '{sheet_id}'")
            return

        bridge = LocalSheetBridge.attach(sheet_id)
        data = bridge.dump()
        link = LINKS[sheet_id]

        payload = {
            "token": link["token"],
            "action": "sync_push",
            "sheet": sheet_id,
            "data": data
        }

        req = urllib.request.Request(
            link["url"],
            data=json.dumps(payload).encode("utf-8"),
            headers={"Content-Type": "application/json"},
        )

        try:
            with urllib.request.urlopen(req) as res:
                result = res.read().decode("utf-8")
                print(f"[LinkSheet] Push → {sheet_id} OK: {result}")
        except Exception as e:
            print(f"[LinkSheet] Error de push: {e}")

    @staticmethod
    def pull(sheet_id):
        """Obtiene contenido remoto desde Google Sheets (si Apps Script lo soporta)."""
        if sheet_id not in LINKS:
            print(f"[LinkSheet] No hay enlace definido para '{sheet_id}'")
            return

        link = LINKS[sheet_id]
        payload = {
            "token": link["token"],
            "action": "sync_pull",
            "sheet": sheet_id
        }

        req = urllib.request.Request(
            link["url"],
            data=json.dumps(payload).encode("utf-8"),
            headers={"Content-Type": "application/json"},
        )

        try:
            with urllib.request.urlopen(req) as res:
                data = json.loads(res.read().decode("utf-8"))
                bridge = LocalSheetBridge.attach(sheet_id)
                bridge.restore(data)
                bridge.save()
                print(f"[LinkSheet] Pull ← {sheet_id} actualizado.")
        except Exception as e:
            print(f"[LinkSheet] Error de pull: {e}")
