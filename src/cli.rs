use std::net::SocketAddr;
use std::net::ToSocketAddrs;

use clap::Parser;

#[derive(Parser, Debug)]
#[clap(author, version, about, long_about = None)]
pub struct Args {
    #[clap(short, long, value_parser)]
    pub bind_address: Option<SocketAddr>,
}
