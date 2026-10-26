use clap::Parser;

#[derive(Debug, Parser)]
#[command(
    name = "ariawatch",
    about = "An aria2 based download manager in the terminal"
)]
pub struct Cli {
    #[arg(long)]
    pub external: bool,

    #[arg(long, hide = true, conflicts_with = "external")]
    pub controller: bool,

    #[arg(long, value_name = "URL", requires = "external")]
    pub rpc_url: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::Cli;
    use clap::Parser;

    #[test]
    fn defaults_to_managed_mode() {
        let cli = Cli::try_parse_from(["ariawatch"]).unwrap();

        assert!(!cli.controller);
        assert!(!cli.external);
        assert_eq!(cli.rpc_url, None);
    }

    #[test]
    fn accepts_external_server_mode() {
        let cli = Cli::try_parse_from([
            "ariawatch",
            "--external",
            "--rpc-url",
            "http://127.0.0.1:6800/jsonrpc",
        ])
        .unwrap();

        assert!(cli.external);
        assert_eq!(
            cli.rpc_url.as_deref(),
            Some("http://127.0.0.1:6800/jsonrpc")
        );
    }

    #[test]
    fn rpc_url_requires_external_mode() {
        let result =
            Cli::try_parse_from(["ariawatch", "--rpc-url", "http://127.0.0.1:6800/jsonrpc"]);

        assert!(result.is_err());
    }

    #[test]
    fn accepts_hidden_controller_mode() {
        let cli = Cli::try_parse_from(["ariawatch", "--controller"]).unwrap();

        assert!(cli.controller);
        assert!(!cli.external);
        assert_eq!(cli.rpc_url, None);
    }

    #[test]
    fn controller_mode_conflicts_with_external_mode() {
        let result = Cli::try_parse_from(["ariawatch", "--controller", "--external"]);

        assert!(result.is_err());
    }

    #[test]
    fn controller_flag_is_hidden_from_help() {
        let error = Cli::try_parse_from(["ariawatch", "--help"]).unwrap_err();

        assert_eq!(error.kind(), clap::error::ErrorKind::DisplayHelp);
        assert!(!error.to_string().contains("--controller"));
    }
}
