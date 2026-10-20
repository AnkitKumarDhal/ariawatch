use std::env;

use crate::cli::Cli;

pub const DEFAULT_RPC_URL: &str = "http://127.0.0.1:6800/jsonrpc";

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RuntimeMode {
    Managed,
    External(ExternalServerConfig),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExternalServerConfig {
    pub rpc_url: String,
    pub rpc_secret: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RuntimeConfig {
    pub mode: RuntimeMode,
}

impl RuntimeConfig {
    pub fn from_cli(cli: Cli) -> Self {
        if !cli.external {
            return Self {
                mode: RuntimeMode::Managed,
            };
        }

        let rpc_url = cli
            .rpc_url
            .or_else(|| env::var("ARIA2_RPC_URL").ok())
            .unwrap_or_else(|| DEFAULT_RPC_URL.to_owned());

        let rpc_secret = env::var("ARIA2_RPC_SECRET").ok();

        Self {
            mode: RuntimeMode::External(ExternalServerConfig {
                rpc_url,
                rpc_secret,
            }),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::Parser;

    #[test]
    fn default_configuration_selects_managed_mode() {
        let cli = Cli::try_parse_from(["ariawatch"]).unwrap();
        let config = RuntimeConfig::from_cli(cli);

        assert_eq!(config.mode, RuntimeMode::Managed);
    }

    #[test]
    fn explicit_external_configuration_is_preserved() {
        let cli = Cli::try_parse_from([
            "ariawatch",
            "--external",
            "--rpc-url",
            "http://127.0.0.1:6800/jsonrpc",
        ])
        .unwrap();

        let config = RuntimeConfig::from_cli(cli);

        match config.mode {
            RuntimeMode::External(external) => {
                assert_eq!(external.rpc_url, "http://127.0.0.1:6800/jsonrpc");
            }
            RuntimeMode::Managed => {
                panic!("Expected external-server configuration");
            }
        }
    }
}
