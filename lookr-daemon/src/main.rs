mod proto;
mod rpc;

use crate::proto::rpc::lookr_server::LookrServer;
use clap::{App, AppSettings, Arg};
use tonic::transport::Server;

static DEFAULT_ADDR: &str = "[::1]:50051";

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let matches = App::new(env!("CARGO_PKG_NAME"))
        .setting(AppSettings::ColoredHelp)
        .version(env!("CARGO_PKG_VERSION"))
        .author(env!("CARGO_PKG_AUTHORS"))
        .about(env!("CARGO_PKG_DESCRIPTION"))
        .arg(
            Arg::with_name("addr")
                .short("a")
                .long("addr")
                .help(
                    format!(
                        "Override the default interface address to bind to: {}",
                        DEFAULT_ADDR
                    )
                    .as_str(),
                )
                .takes_value(true)
                .required(false)
                .global(true),
        )
        .arg(
            Arg::with_name("config")
                .short("c")
                .long("config")
                .help("Specify a config file")
                .takes_value(true)
                .required(false)
                .global(true),
        )
        .get_matches();
    
    let addr = matches.value_of("addr").unwrap_or(DEFAULT_ADDR).parse()?;
    let lookr = rpc::LookrService::new();

    Server::builder().add_service(LookrServer::new(lookr)).serve(addr).await?;
    
    Ok(())
}
