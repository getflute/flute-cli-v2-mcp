pub mod config;
pub mod error;
pub mod runner;
pub mod server;
pub mod tools;

#[cfg(test)]
mod tests {
    #[test]
    fn package_and_lib_are_named_for_the_v2_cli() {
        assert_eq!(env!("CARGO_PKG_NAME"), "flute2-mcp");
        assert_eq!(module_path!().split("::").next().unwrap(), "flute2_mcp");
    }
}
