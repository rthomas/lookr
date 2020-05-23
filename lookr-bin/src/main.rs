use clap::{App, AppSettings, Arg};
use lookrd::proto::rpc::lookr_client::LookrClient;
use lookrd::proto::rpc::QueryReq;
use std::error;
use tonic::Request;

static DEFAULT_SERVER: &str = "[::1]:50051";

#[tokio::main]
async fn main() -> Result<(), Box<dyn error::Error>> {
    let matches = App::new(env!("CARGO_PKG_NAME"))
        .setting(AppSettings::ColoredHelp)
        .version(env!("CARGO_PKG_VERSION"))
        .author(env!("CARGO_PKG_AUTHORS"))
        .about(env!("CARGO_PKG_DESCRIPTION"))
        .arg(
            Arg::with_name("QUERY")
                .help("The query to run against the index.")
                .required(true)
                .index(1),
        )
        .arg(
            Arg::with_name("addr")
                .short("a")
                .long("addr")
                .help(
                    format!(
                        "Override the default server to connect to: {}",
                        DEFAULT_SERVER
                    )
                    .as_str(),
                )
                .takes_value(true)
                .required(false)
                .global(true),
        )
        .get_matches();

    let query = matches.value_of("QUERY").unwrap();

    let server = matches.value_of("addr").unwrap_or(DEFAULT_SERVER);
    let mut client = LookrClient::connect(format!("http://{}", server)).await?;

    let req = Request::new(QueryReq {
        query: query.to_string(),
        count: 0,
        offset: 0,
    });

    let resp = client.query(req).await?;

    for r in &resp.get_ref().results {
        println!("Result: {}", r);
    }

    Ok(())
}
