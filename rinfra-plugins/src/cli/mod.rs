pub mod client;
pub mod handlers;
pub mod runner;

use clap::{Parser, Subcommand, ValueEnum};
use rinfra_core::cli::OutputFormat as CoreOutputFormat;

#[derive(Parser, Debug)]
#[command(name = "rinfra", version, about = "rinfra unified backend infrastructure")]
pub struct Cli {
    #[command(subcommand)]
    pub command: Option<Commands>,

    /// Path to the configuration file
    #[arg(long, default_value = "config/standalone.example.yaml", global = true)]
    pub config: String,

    /// Output format
    #[arg(long, default_value = "pretty", global = true)]
    pub format: CliOutputFormat,
}

#[derive(Subcommand, Debug)]
pub enum Commands {
    /// Start the server (default if no subcommand given)
    Server(ServerArgs),
    /// Configuration management
    Config(ConfigArgs),
    /// Plugin management
    Plugin(PluginArgs),
    /// Local cache operations
    Cache(CacheArgs),
    /// Cluster remote commands (sent to master via HTTP)
    Cluster(ClusterArgs),
    /// Business-defined subcommands (handled by extra_commands callback)
    #[command(external_subcommand)]
    External(Vec<String>),
}

// -- server --

#[derive(Parser, Debug)]
pub struct ServerArgs {
    #[command(subcommand)]
    pub command: ServerCommands,
}

#[derive(Subcommand, Debug)]
pub enum ServerCommands {
    /// Start the server
    Start,
}

// -- config --

#[derive(Parser, Debug)]
pub struct ConfigArgs {
    #[command(subcommand)]
    pub command: ConfigCommands,
}

#[derive(Subcommand, Debug)]
pub enum ConfigCommands {
    /// Show current effective configuration (use --defaults to show full defaults as YAML)
    Show(ConfigShowArgs),
    /// Validate configuration file
    Validate,
    /// Generate a reference configuration file with full comments
    Init(ConfigInitArgs),
}

#[derive(Parser, Debug)]
pub struct ConfigShowArgs {
    /// Output all fields with their default values as YAML (ignores --format)
    #[arg(long)]
    pub defaults: bool,
}

#[derive(Parser, Debug)]
pub struct ConfigInitArgs {
    /// Output file path (default: ./rinfra.standalone.yaml)
    #[arg(short, long, default_value = "rinfra.standalone.yaml")]
    pub output: String,

    /// Overwrite if file already exists
    #[arg(long)]
    pub force: bool,
}

// -- plugin --

#[derive(Parser, Debug)]
pub struct PluginArgs {
    #[command(subcommand)]
    pub command: PluginCommands,
}

#[derive(Subcommand, Debug)]
pub enum PluginCommands {
    /// List all plugins and their status
    List,
}

// -- cache (local) --

#[derive(Parser, Debug)]
pub struct CacheArgs {
    #[command(subcommand)]
    pub command: CacheCommands,
}

#[derive(Subcommand, Debug)]
pub enum CacheCommands {
    /// Get a value from local cache
    Get {
        /// Cache key to look up
        key: String,
    },
    /// Flush all entries from local cache
    Flush,
}

// -- cluster (remote) --

#[derive(Parser, Debug)]
pub struct ClusterArgs {
    /// Override target master address (default: from config cluster.main_address)
    #[arg(long, global = true)]
    pub target: Option<String>,

    #[command(subcommand)]
    pub command: ClusterCommands,
}

#[derive(Subcommand, Debug)]
pub enum ClusterCommands {
    /// List cluster nodes
    Nodes,
    /// Show cluster health and info
    Info,
    /// Cluster-wide cache operations
    Cache(ClusterCacheArgs),
}

#[derive(Parser, Debug)]
pub struct ClusterCacheArgs {
    #[command(subcommand)]
    pub command: ClusterCacheCommands,
}

#[derive(Subcommand, Debug)]
pub enum ClusterCacheCommands {
    /// Get a value from cluster cache (via master)
    Get {
        /// Cache key to look up
        key: String,
    },
    /// Flush cluster-wide cache (via master)
    Flush {
        /// Skip confirmation
        #[arg(long)]
        yes: bool,
    },
}

// -- output format --

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub enum CliOutputFormat {
    Pretty,
    Json,
}

impl From<CliOutputFormat> for CoreOutputFormat {
    fn from(f: CliOutputFormat) -> Self {
        match f {
            CliOutputFormat::Pretty => CoreOutputFormat::Pretty,
            CliOutputFormat::Json => CoreOutputFormat::Json,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::CommandFactory;

    #[test]
    fn test_cli_parses_server_start() {
        let cli = Cli::parse_from(["rinfra", "server", "start"]);
        assert!(matches!(cli.command, Some(Commands::Server(_))));
        if let Some(Commands::Server(args)) = cli.command {
            assert!(matches!(args.command, ServerCommands::Start));
        }
    }

    #[test]
    fn test_cli_no_subcommand_is_none() {
        let cli = Cli::parse_from(["rinfra"]);
        assert!(cli.command.is_none());
    }

    #[test]
    fn test_cli_parses_plugin_list_json() {
        let cli = Cli::parse_from(["rinfra", "--format", "json", "plugin", "list"]);
        assert_eq!(cli.format, CliOutputFormat::Json);
        assert!(matches!(cli.command, Some(Commands::Plugin(_))));
    }

    #[test]
    fn test_cli_parses_config_show() {
        let cli = Cli::parse_from(["rinfra", "config", "show"]);
        if let Some(Commands::Config(args)) = cli.command {
            if let ConfigCommands::Show(show_args) = args.command {
                assert!(!show_args.defaults);
            } else {
                panic!("expected ConfigCommands::Show");
            }
        } else {
            panic!("expected Commands::Config");
        }
    }

    #[test]
    fn test_cli_parses_config_show_defaults() {
        let cli = Cli::parse_from(["rinfra", "config", "show", "--defaults"]);
        if let Some(Commands::Config(args)) = cli.command {
            if let ConfigCommands::Show(show_args) = args.command {
                assert!(show_args.defaults);
            } else {
                panic!("expected ConfigCommands::Show");
            }
        } else {
            panic!("expected Commands::Config");
        }
    }

    #[test]
    fn test_cli_parses_config_init() {
        let cli = Cli::parse_from(["rinfra", "config", "init"]);
        if let Some(Commands::Config(args)) = cli.command {
            if let ConfigCommands::Init(init_args) = args.command {
                assert_eq!(init_args.output, "rinfra.standalone.yaml");
                assert!(!init_args.force);
            } else {
                panic!("expected ConfigCommands::Init");
            }
        } else {
            panic!("expected Commands::Config");
        }
    }

    #[test]
    fn test_cli_parses_config_init_with_options() {
        let cli = Cli::parse_from(["rinfra", "config", "init", "-o", "my.yaml", "--force"]);
        if let Some(Commands::Config(args)) = cli.command {
            if let ConfigCommands::Init(init_args) = args.command {
                assert_eq!(init_args.output, "my.yaml");
                assert!(init_args.force);
            } else {
                panic!("expected ConfigCommands::Init");
            }
        } else {
            panic!("expected Commands::Config");
        }
    }

    #[test]
    fn test_cli_parses_config_validate() {
        let cli = Cli::parse_from(["rinfra", "config", "validate"]);
        if let Some(Commands::Config(args)) = cli.command {
            assert!(matches!(args.command, ConfigCommands::Validate));
        } else {
            panic!("expected Commands::Config");
        }
    }

    #[test]
    fn test_cli_parses_cache_get() {
        let cli = Cli::parse_from(["rinfra", "cache", "get", "my-key"]);
        if let Some(Commands::Cache(args)) = cli.command {
            if let CacheCommands::Get { key } = args.command {
                assert_eq!(key, "my-key");
            } else {
                panic!("expected CacheCommands::Get");
            }
        } else {
            panic!("expected Commands::Cache");
        }
    }

    #[test]
    fn test_cli_parses_cache_flush() {
        let cli = Cli::parse_from(["rinfra", "cache", "flush"]);
        assert!(matches!(cli.command, Some(Commands::Cache(CacheArgs { command: CacheCommands::Flush }))));
    }

    #[test]
    fn test_cli_parses_cluster_nodes() {
        let cli = Cli::parse_from(["rinfra", "cluster", "nodes"]);
        if let Some(Commands::Cluster(args)) = cli.command {
            assert!(args.target.is_none());
            assert!(matches!(args.command, ClusterCommands::Nodes));
        } else {
            panic!("expected Commands::Cluster");
        }
    }

    #[test]
    fn test_cli_parses_cluster_with_target() {
        let cli = Cli::parse_from(["rinfra", "cluster", "--target", "192.168.1.10:8089", "nodes"]);
        if let Some(Commands::Cluster(args)) = cli.command {
            assert_eq!(args.target, Some("192.168.1.10:8089".to_string()));
        } else {
            panic!("expected Commands::Cluster");
        }
    }

    #[test]
    fn test_cli_parses_cluster_cache_get() {
        let cli = Cli::parse_from(["rinfra", "cluster", "cache", "get", "foo"]);
        if let Some(Commands::Cluster(args)) = cli.command {
            if let ClusterCommands::Cache(cache_args) = args.command {
                assert!(matches!(cache_args.command, ClusterCacheCommands::Get { key } if key == "foo"));
            } else {
                panic!("expected ClusterCommands::Cache");
            }
        }
    }

    #[test]
    fn test_cli_parses_cluster_cache_flush() {
        let cli = Cli::parse_from(["rinfra", "cluster", "cache", "flush", "--yes"]);
        if let Some(Commands::Cluster(args)) = cli.command {
            if let ClusterCommands::Cache(cache_args) = args.command {
                assert!(matches!(cache_args.command, ClusterCacheCommands::Flush { yes: true }));
            } else {
                panic!("expected ClusterCommands::Cache");
            }
        }
    }

    #[test]
    fn test_cli_default_config_path() {
        let cli = Cli::parse_from(["rinfra", "server", "start"]);
        assert_eq!(cli.config, "config/standalone.example.yaml");
    }

    #[test]
    fn test_cli_custom_config_path() {
        let cli = Cli::parse_from(["rinfra", "--config", "my.yaml", "server", "start"]);
        assert_eq!(cli.config, "my.yaml");
    }

    #[test]
    fn test_cli_verify_command() {
        Cli::command().debug_assert();
    }
}
