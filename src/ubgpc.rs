use ubgp::config_client::ConfigClient;
use ubgp::NeighborRequest;

pub mod ubgp {
    tonic::include_proto!("ubgp");
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut client = ConfigClient::connect("http://[::1]:50051").await?;

    let request = tonic::Request::new(NeighborRequest { ip: "Tonic".into() });

    let response = client.get_neighbor_config(request).await?;

    println!("RESPONSE={:?}", response);

    Ok(())
}
