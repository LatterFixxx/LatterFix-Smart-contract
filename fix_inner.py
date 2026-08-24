import glob
import os

for filepath in glob.glob("src/**/*.rs", recursive=True):
    with open(filepath, "r") as f:
        lines = f.readlines()
        
    if not lines:
        continue
        
    if lines[0].strip() == "use soroban_sdk::unwrap::UnwrapOptimized;":
        # Check if subsequent lines are inner attributes
        inner_attrs = []
        idx = 1
        while idx < len(lines):
            line = lines[idx].strip()
            if line.startswith("#!"):
                inner_attrs.append(lines[idx])
                idx += 1
            elif not line: # skip blank lines
                inner_attrs.append(lines[idx])
                idx += 1
            else:
                break
                
        if inner_attrs and any(l.strip().startswith("#!") for l in inner_attrs):
            new_lines = inner_attrs + [lines[0]] + lines[idx:]
            with open(filepath, "w") as f:
                f.writelines(new_lines)
            print(f"Fixed {filepath}")

