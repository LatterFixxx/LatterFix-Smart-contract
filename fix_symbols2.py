import os
import re

def fix_symbol(match):
    prefix = match.group(1)
    text = match.group(2)
    # normalize text to alphanumeric and underscore
    text = re.sub(r'[^a-zA-Z0-9_]', '_', text)
    # truncate to 32 chars
    if not text:
        text = "empty"
    text = text[:32]
    return f'{prefix}"{text}")'

for file in os.listdir("src"):
    if file.endswith(".rs"):
        path = os.path.join("src", file)
        with open(path, "r") as f:
            content = f.read()
        
        # Match Symbol::new(ANYTHING, "STRING")
        content = re.sub(r'(Symbol::new\s*\(\s*[^,]+\s*,\s*)"([^"]*)"\)', fix_symbol, content)
        
        with open(path, "w") as f:
            f.write(content)

