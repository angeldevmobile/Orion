""" Random futurista en Orion. """
import random, uuid

def int(a, b): return random.randint(a, b)
def float(): return random.random()
def choice(seq): return random.choice(seq)
def shuffle(seq): random.shuffle(seq); return seq
def uuidv4(): return str(uuid.uuid4())
