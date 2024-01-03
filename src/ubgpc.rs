use clap::{Parser, Subcommand};
use ubgp::config_client::ConfigClient;
use ubgp::state_client::StateClient;
use ubgp::NeighborRequest;
use ubgp::RibRequest;

pub mod ubgp {
    tonic::include_proto!("ubgp");
}

#[derive(Parser)]
#[command(author, version, about, long_about = None)]
struct Opt {
    #[arg(short, long, default_value = "127.0.0.1")]
    server: String,

    #[arg(short, long, default_value_t = 50051)]
    port: u16,

    #[command(subcommand)]
    command: Option<Commands>,
}

#[derive(Subcommand)]
enum Commands {
    Rib(RibArgs),
    Neighbors(NeighborsArgs),
}

#[derive(clap::Args)]
#[command(author, version, about, long_about = None)]
struct RibArgs {
    #[arg(short, long, value_parser, default_value_t = 1)]
    afi: u32,
    #[arg(short, long, value_parser, default_value_t = 1)]
    safi: u32,
}

#[derive(clap::Args)]
#[command(author, version, about, long_about = None)]
struct NeighborsArgs {
    #[arg(short, long, value_parser)]
    address: Option<String>,
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let opt = Opt::parse();

    match opt.command {
        None => {}
        Some(Commands::Rib(rib_args)) => {
            let mut client =
                StateClient::connect(format!("http://{}:{}", opt.server, opt.port)).await?;
            let request = tonic::Request::new(RibRequest {
                afi: rib_args.afi,
                safi: rib_args.safi,
            });
            let response = client.get_rib(request).await?;
            println!("{:?}", response.get_ref());
        }
        Some(Commands::Neighbors(neighbors_args)) => {
            let mut client =
                ConfigClient::connect(format!("http://{}:{}", opt.server, opt.port)).await?;
            let request = tonic::Request::new(NeighborRequest {
                ip: neighbors_args.address,
            });
            let response = client.get_neighbor_config(request).await?;
            println!("{:?}", response.get_ref());
        }
    }

    Ok(())
}
