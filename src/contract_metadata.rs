//! Discoverable, build-time Soroban metadata and a stable read-only query.
//!
//! SEP-46 (not SEP-34) defines the `contractmetav0` Wasm section. Keep
//! the Rust metadata entries and the JSON query below aligned with Cargo.toml.
//! This describes the contract CODE; it is not a claim about a deployment.

pub const CONTRACT_VERSION: &str = "0.2.0";
pub const CONTRACT_METADATA_JSON: &str = r#"{"name":"TaskManager Pro","description":"Soroban task management, escrow, governance and reputation","author":"LatterFix Team","version":"0.2.0","source_repo":"https://github.com/LatterFixxx/LatterFix-Smart-contract"}"#;

soroban_sdk::contractmeta!(key = "name", val = "TaskManager Pro");
soroban_sdk::contractmeta!(
    key = "description",
    val = "Soroban task management, escrow, governance and reputation"
);
soroban_sdk::contractmeta!(key = "author", val = "LatterFix Team");
soroban_sdk::contractmeta!(key = "version", val = "0.2.0");
soroban_sdk::contractmeta!(
    key = "source_repo",
    val = "https://github.com/LatterFixxx/LatterFix-Smart-contract"
);
