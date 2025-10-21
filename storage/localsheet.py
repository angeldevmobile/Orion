"""
Proxy de LocalSheetBridge
Redirige las operaciones del protocolo OSync al módulo completo
'sheets_localbridge' dentro de 'modules'.
"""

from modules.sheets_localbridge import LocalSheetBridge as FullBridge


class LocalSheetBridge:
    """Proxy hacia el Orion LocalSheet Bridge real."""
    
    @classmethod
    def register(cls, id_name, path):
        return FullBridge.register(id_name, path)
    
    @classmethod
    def attach(cls, id_name):
        return FullBridge.attach(id_name)
