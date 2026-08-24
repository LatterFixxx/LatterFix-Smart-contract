import os

filepath = "src/vesting_vault_test.rs"
with open(filepath, "r") as f:
    content = f.read()

content = content.replace("String::from_str", "Symbol::new")
content = content.replace("Vec<String>", "Vec<Symbol>")
content = content.replace("soroban_sdk::String", "soroban_sdk::Symbol")

with open(filepath, "w") as f:
    f.write(content)

