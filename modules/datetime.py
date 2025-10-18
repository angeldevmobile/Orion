""" Fechas y tiempos futuristas en Orion. """
from datetime import datetime, timedelta

def now():
    return datetime.now().isoformat()

def today():
    return datetime.today().date().isoformat()

def format(dt, fmt="%Y-%m-%d %H:%M:%S"):
    return dt.strftime(fmt)

def parse(s, fmt="%Y-%m-%d %H:%M:%S"):
    return datetime.strptime(s, fmt)

def add_days(dt, days):
    return dt + timedelta(days=days)
