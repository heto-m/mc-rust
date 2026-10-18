//! The loader builds mcr runs. `xtask` compiles this file too (`#[path]`), so it holds constants only.

/// Fabric Loader, fetched by `cargo xtask fabric-fetch`.
pub const LOADER_VERSION: &str = "0.19.5";
/// NeoForge, fetched by `cargo xtask neoforge-fetch`, and the FancyModLoader it ships.
pub const NEOFORGE_VERSION: &str = "26.3.0.90";
pub const FML_VERSION: &str = "12.0.12";
