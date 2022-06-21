use std::net::SocketAddr;
use std::net::ToSocketAddrs;

use clap::{Parser, ValueEnum};

#[derive(ValueEnum, Clone, Debug)]
pub enum KeyringOption {
    Default,
    Custom,
}

/// Run commands based on optionally signed commits from a Git repository.
#[derive(Parser, Debug)]
#[clap(author, version, about, long_about = None)]
pub struct Args {
    /// Address to bind to; only accepts one argument, for multiple bind addresses use a reverse
    /// proxy
    #[clap(short, long, env, value_parser, default_value = "0.0.0.0:80")]
    pub bind_address: SocketAddr,

    /// Remote address of the Git repository; supports any format Git supports, such as
    /// `git@github.com:RyanSquared/webhook-runner`
    #[clap(long, env, value_parser)]
    pub git_repository: Option<String>,

    /// TEMP: Command to run when receiving any webhook
    #[clap(value_parser)]
    pub command: String,

    /// TEMP: Optional arguments passed to command when receiving any webhook
    #[clap(value_parser)]
    pub arguments: Vec<String>,

    /// UNSTABLE: PGP keyring file for verifying commits
    #[clap(long, env, value_parser)]
    pub commit_keyring: Option<String>,

    /// UNSTABLE: Shell command to run after commits are (optionally) verified
    #[clap(long, env, value_parser)]
    pub commit_command: Option<String>,

    /// UNSTABLE: PGP keyring file for verifying tags
    #[clap(long, env, value_parser)]
    pub tag_keyring: Option<String>,

    /// UNSTABLE: Shell command to run after tags are (optionally) verified
    #[clap(long, env, value_parser)]
    pub tag_command: Option<String>,

    /// UNSTABLE: Timeout for `git clone` in seconds
    // Annoyingly, I can't just do default_value = u32::MAX
    #[clap(long, env, default_value = "4294967295", value_parser)]
    pub clone_timeout: u32,

    /// UNSTABLE: Timeout for commands run by webhooks in seconds
    // TODO: Unused.
    #[clap(long, env, default_value = "4294967295", value_parser)]
    pub command_timeout: u32,
}

impl Args {
    /// Determine whether or not the configuration passed to the program is correct; for example,
    /// whether or not commands were defined for every variant that also has a keyring.
    pub fn assert(self) -> Self {
        if self.tag_keyring.is_some() {
            assert!(
                self.tag_command.is_some(),
                "tag keyring defined without defining tag command"
            );
        }
        if self.commit_keyring.is_some() {
            assert!(
                self.commit_command.is_some(),
                "commit keyring defined without defining commit command"
            );
        }
        self
    }
}
