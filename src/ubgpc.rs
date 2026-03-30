use anyhow::Result;
use clap::{Args, Parser, Subcommand};
use ubgp::{config_client::ConfigClient, state_client::StateClient, NeighborRequest, RibRequest};

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

#[derive(Args)]
struct RibArgs {
    #[arg(short, long, default_value_t = 1)]
    afi: u32,
    #[arg(short, long, default_value_t = 1)]
    safi: u32,
}

#[derive(Args)]
struct NeighborsArgs {
    #[arg(short, long)]
    address: Option<String>,
}

#[tokio::main]
async fn main() -> Result<()> {
    let opt = Opt::parse();

    let command = match opt.command {
        Some(cmd) => cmd,
        None => {
            println!("Please provide a command. Use --help for usage information.");
            std::process::exit(1);
        }
    };

    let server_url = format!("http://{}:{}", opt.server, opt.port);

    match command {
        Commands::Rib(args) => {
            let mut client = StateClient::connect(server_url).await?;
            let request = tonic::Request::new(RibRequest {
                afi: args.afi,
                safi: args.safi,
            });
            let response = client.get_rib(request).await?;
            println!("{:?}", response.get_ref());
        }
        Commands::Neighbors(args) => {
            let mut client = ConfigClient::connect(server_url).await?;
            let request = tonic::Request::new(NeighborRequest { ip: args.address });
            let response = client.get_neighbor_config(request).await?;
            println!("{:?}", response.get_ref());
        }
    }

    Ok(())
}
