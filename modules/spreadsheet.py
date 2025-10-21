"""
Módulo spreadsheet de Orion
Permite crear, leer y escribir hojas de cálculo (.xlsx, .csv, .orion)
sin depender de APIs externas, y preparado para sincronización Orion Cloud.
"""

import os
import csv
import json
from openpyxl import Workbook, load_workbook
from modules import net  # Usa tu propio módulo de red Orion


class OrionSpreadsheet:
    """Controlador universal de hojas de cálculo Orion."""
    _registry = {}  # para manejar identificadores (IDs) internos

    def __init__(self, filename):
        self.filename = filename
        self.ext = os.path.splitext(filename)[1].lower()

        # --- Soporte local ---
        if self.ext == ".xlsx":
            if os.path.exists(filename):
                self.wb = load_workbook(filename)
                self.ws = self.wb.active
            else:
                self.wb = Workbook()
                self.ws = self.wb.active

        elif self.ext == ".csv":
            self.rows = []
            if os.path.exists(filename):
                with open(filename, newline='', encoding='utf-8') as f:
                    self.rows = list(csv.reader(f))

        # --- Formato nativo Orion ---
        elif self.ext in [".orion", ".osheet"]:
            if os.path.exists(filename):
                with open(filename, "r", encoding="utf-8") as f:
                    self.data = json.load(f)
            else:
                self.data = {"headers": [], "rows": []}

        else:
            raise ValueError(f"Formato no soportado: {self.ext}")

    # -----------------------------------------------------------
    # Operaciones básicas
    # -----------------------------------------------------------

    def write(self, cell, value):
        if self.ext == ".xlsx":
            self.ws[cell] = value
        elif self.ext in [".orion", ".osheet"]:
            # escritura simbólica tipo A1, B2, etc.
            row_idx = int(''.join(filter(str.isdigit, cell))) - 1
            col_letter = ''.join(filter(str.isalpha, cell)).upper()
            col_idx = ord(col_letter) - 65  # A → 0
            while len(self.data["rows"]) <= row_idx:
                self.data["rows"].append([])
            row = self.data["rows"][row_idx]
            while len(row) <= col_idx:
                row.append("")
            row[col_idx] = value
        else:
            raise TypeError("La escritura directa solo aplica a .xlsx o .orion")

    def append(self, values):
        if self.ext == ".xlsx":
            self.ws.append(values)
        elif self.ext == ".csv":
            self.rows.append(values)
        elif self.ext in [".orion", ".osheet"]:
            self.data["rows"].append(values)

    def read(self, cell):
        if self.ext == ".xlsx":
            return self.ws[cell].value
        elif self.ext in [".orion", ".osheet"]:
            row_idx = int(''.join(filter(str.isdigit, cell))) - 1
            col_letter = ''.join(filter(str.isalpha, cell)).upper()
            col_idx = ord(col_letter) - 65
            try:
                return self.data["rows"][row_idx][col_idx]
            except IndexError:
                return None
        else:
            raise TypeError("Lectura no soportada para este formato")

    def save(self):
        if self.ext == ".xlsx":
            self.wb.save(self.filename)
        elif self.ext == ".csv":
            with open(self.filename, "w", newline="", encoding="utf-8") as f:
                writer = csv.writer(f)
                writer.writerows(self.rows)
        elif self.ext in [".orion", ".osheet"]:
            with open(self.filename, "w", encoding="utf-8") as f:
                json.dump(self.data, f, indent=2, ensure_ascii=False)
        return f"Archivo guardado: {self.filename}"

    # -----------------------------------------------------------
    # Sincronización Cloud
    # -----------------------------------------------------------

    def sync(self, endpoint):
        """Sincroniza la hoja con un endpoint Orion Cloud."""
        if self.ext in [".orion", ".osheet"]:
            payload = {"sheet": self.data, "source": self.filename}
        else:
            payload = {"message": f"Sync desde {self.filename}"}

        try:
            res = net.transmit(endpoint, json_data=payload)
            if res.status in [200, 201]:
                return f"Sincronización exitosa con {endpoint}"
            else:
                return f"Error de sincronización: {res.status}"
        except Exception as e:
            return f"Fallo al conectar con Orion Cloud: {e}"

    # -----------------------------------------------------------
    # Gestión de registros / IDs Orion
    # -----------------------------------------------------------

    @classmethod
    def register(cls, id_name, filename):
        cls._registry[id_name] = filename
        return f"Hoja registrada con ID '{id_name}' → {filename}"

    @classmethod
    def attach(cls, id=None, filename=None):
        if not filename and id in cls._registry:
            filename = cls._registry[id]
        if not filename:
            raise ValueError("Debe especificarse un filename o un ID válido")
        return OrionSpreadsheet(filename)


# -----------------------------------------------------------
# API expuesta a Orion
# -----------------------------------------------------------

def create(filename):
    return OrionSpreadsheet(filename)

def attach(id=None, filename=None):
    return OrionSpreadsheet.attach(id=id, filename=filename)

def register(id_name, filename):
    return OrionSpreadsheet.register(id_name, filename)
