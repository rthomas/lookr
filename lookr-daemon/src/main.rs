mod index;
mod indexer;
mod proto;
mod rpc;

#[macro_use]
extern crate log;

use crate::proto::rpc::lookr_server::LookrServer;
use clap::{App, AppSettings, Arg};
use serde::{Deserialize, Serialize};
use std::fs::File;
use std::io::{self, BufReader};
use std::path::Path;
use std::sync::{Arc, Mutex};
use std::thread;
use tonic::transport::Server;

static DEFAULT_ADDR: &str = "[::1]:50051";
static DEFAULT_CONFIG: &str = ".lookrd";

#[derive(Clone, Debug, Deserialize, Serialize)]
struct LookrdConfig {
    /// The paths that will be indexed by the indexer.
    index_paths: Vec<String>,
    /// The location this data will be written to.
    data_dir: String,
    // Optional list of users to generate secrets for, if not provided will
    // generate them for all users.
    users: Option<String>,
}

fn read_config(cfg: &Path) -> io::Result<LookrdConfig> {
    let reader = BufReader::new(File::open(cfg)?);
    let config = serde_json::from_reader(reader)?;
    Ok(config)
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    pretty_env_logger::init();

    info!("{} v{}", env!("CARGO_PKG_NAME"), env!("CARGO_PKG_VERSION"));

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
    let config = match matches.value_of("config") {
        Some(c) => read_config(Path::new(c))?,
        None => {
            let mut home = dirs::home_dir().expect("No home directory found...");
            home.push(DEFAULT_CONFIG);
            read_config(home.as_path())?
        }
    };

    // Plan: What needs to happen before we index things:
    // 1. We need to get a list of the users on the system
    // 2. We generate a user read-only sercret key for them
    // 3. Add an endpoint for a request to get a local path for the key for a given user
    // 4. Add the key requirement to the query to authenticate the request.
    // 5. Also index the file permissions to make sure we filter the correct files out.

    info!("Creating index");
    let index = Arc::new(Mutex::new(index::Index::new()));
    let idx_clone = index.clone();

    info!("Starting indexer thread");
    let idx_thread = thread::spawn(move || {
        let mut paths = Vec::with_capacity(config.index_paths.len());
        for p in &config.index_paths {
            paths.push(Path::new(p));
        }
        let mut indexer = indexer::Indexer::new(idx_clone, &paths).unwrap();
        indexer
            .index()
            .expect("Indexer thread terminating on error");
    });

    info!("Starting RPC server");
    // RPC service and server.
    let lookr = rpc::LookrService::new(index.clone());
    Server::builder()
        .add_service(LookrServer::new(lookr))
        .serve(addr)
        .await?;

    idx_thread.join().expect("Could not join indexer thread");

    Ok(())
}
