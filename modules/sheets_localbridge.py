"""
Orion LocalSheet Bridge (OLSB)
────────────────────────────────────────────
Sistema de hojas de cálculo universal sin API externa.
Lee y escribe en archivos locales, y prepara sincronización
inteligente hacia Orion Cloud o nodos remotos.

Formato universal: orion://sheet/<id>/<celda>
"""

import os
import time
import threading
from openpyxl import load_workbook, Workbook
import csv

SYNC_PATH = "storage/sync"

# ============================================================
# OSync Core: Protocolo Orion Sync
# ============================================================

class OSync:
    """Manejador del protocolo Orion Sync (OSync)."""
    @staticmethod
    def push(sheet_id):
        """Sincroniza los cambios locales hacia la nube (futuro)."""
        cache = os.path.join(SYNC_PATH, f"{sheet_id}.orioncache")
        if os.path.exists(cache):
            print(f"[OSync] Pushing cached changes for {sheet_id} ...")
            # futuro: subir al daemon Orion Cloud
        else:
            print(f"[OSync] No changes to push for {sheet_id}.")

    @staticmethod
    def pull(sheet_id):
        """Obtiene actualizaciones desde la nube (futuro)."""
        print(f"[OSync] Pulling updates for {sheet_id} ... (not implemented yet)")

    @staticmethod
    def log(sheet_id, action, cell, value):
        """Registra una acción en cache local."""
        os.makedirs(SYNC_PATH, exist_ok=True)
        cache_file = os.path.join(SYNC_PATH, f"{sheet_id}.orioncache")
        with open(cache_file, "a", encoding="utf-8") as f:
            timestamp = time.strftime("%Y-%m-%d %H:%M:%S")
            f.write(f"[{timestamp}] {action.upper()} {cell} = {value}\n")


# ============================================================
# Bridge: conexión local a hojas de cálculo
# ============================================================

class LocalSheetBridge:
    """Puente local que permite acceso a hojas de cálculo sin API."""
    _registry = {}

    def __init__(self, sheet_id, path):
        self.sheet_id = sheet_id
        self.path = path
        self.ext = os.path.splitext(path)[1].lower()
        self._load()

    def _load(self):
        if self.ext == ".xlsx":
            if os.path.exists(self.path):
                self.wb = load_workbook(self.path)
                self.ws = self.wb.active
            else:
                self.wb = Workbook()
                self.ws = self.wb.active
        elif self.ext == ".csv":
            self.rows = []
            if os.path.exists(self.path):
                with open(self.path, newline="", encoding="utf-8") as f:
                    reader = csv.reader(f)
                    self.rows = list(reader)
        else:
            raise ValueError(f"Formato no soportado: {self.ext}")

    def read(self, cell):
        if self.ext == ".xlsx":
            return self.ws[cell].value
        elif self.ext == ".csv":
            row, col = self._parse_csv_cell(cell)
            return self.rows[row][col]

    def write(self, cell, value):
        if self.ext == ".xlsx":
            self.ws[cell] = value
        elif self.ext == ".csv":
            row, col = self._parse_csv_cell(cell)
            while len(self.rows) <= row:
                self.rows.append([])
            while len(self.rows[row]) <= col:
                self.rows[row].append("")
            self.rows[row][col] = value
        OSync.log(self.sheet_id, "write", cell, value)

    def append(self, values):
        if self.ext == ".xlsx":
            self.ws.append(values)
        elif self.ext == ".csv":
            self.rows.append(values)
        OSync.log(self.sheet_id, "append", "-", values)

    def save(self):
        if self.ext == ".xlsx":
            self.wb.save(self.path)
        elif self.ext == ".csv":
            with open(self.path, "w", newline="", encoding="utf-8") as f:
                writer = csv.writer(f)
                writer.writerows(self.rows)
        print(f"[Bridge] {self.sheet_id} saved → {self.path}")

    def _parse_csv_cell(self, cell):
        # Convierte 'A1' a (fila, columna)
        col = ord(cell[0].upper()) - 65
        row = int(cell[1:]) - 1
        return row, col

    # ============================================================
    # Registro Orion
    # ============================================================

    @classmethod
    def register(cls, id_name, path):
        cls._registry[id_name] = path
        return f"Hoja registrada con ID '{id_name}' → {path}"

    @classmethod
    def attach(cls, id_name):
        if id_name not in cls._registry:
            raise ValueError(f"ID no registrado: {id_name}")
        return LocalSheetBridge(id_name, cls._registry[id_name])


# ============================================================
# API pública Orion
# ============================================================

def register(id_name, path):
    return LocalSheetBridge.register(id_name, path)

def attach(id_name):
    return LocalSheetBridge.attach(id_name)

def push(id_name):
    return OSync.push(id_name)

def pull(id_name):
    return OSync.pull(id_name)
