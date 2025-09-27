import json

def orion_json_parse(text):
    return json.loads(text)

def orion_json_stringify(obj):
    return json.dumps(obj)
