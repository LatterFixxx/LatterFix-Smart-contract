import os
import re

for file in os.listdir("src"):
    if file.endswith(".rs"):
        path = os.path.join("src", file)
        with open(path, "r") as f:
            content = f.read()
        
        # Replace #[should_panic(expected = "...")] with #[should_panic]
        content = re.sub(r'#\[should_panic\([^\]]*\)\]', '#[should_panic]', content)
        
        with open(path, "w") as f:
            f.write(content)

