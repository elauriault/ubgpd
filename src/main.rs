// use serde::Deserialize;
use serde_derive::Deserialize;
use std::error::Error;
// use std::fs::File;
use std::io::prelude::*;
use std::net::Ipv4Addr;
use structopt::StructOpt;
use tokio::net::TcpListener;

#[macro_use]
extern crate derive_builder;

mod bgp;
mod speaker;

#[derive(Deserialize, Debug)]
struct Config {
    asn: u16,
    rid: Ipv4Addr,
    localip: Option<Ipv4Addr>,
    holdtime: Option<u16>,
    port: Option<u16>,
    neighbors: Option<Vec<Neighbor>>,
}

#[derive(Deserialize, Debug)]
struct Neighbor {
    asn: u16,
    ip: String,
    connect_retry: Option<u16>,
    holdtime: Option<u16>,
    keepalive_interval: Option<u16>,
}

#[derive(Debug, StructOpt)]
#[structopt(name = "ubgpd", about = "A minimalistic bgp daemon written in rust.")]
struct Opt {
    #[structopt(short = "c", long = "config", default_value = "ubgpd.conf")]
    config: String,
    // #[structopt(short = "a", long = "asn", default_value = "42")]
    // asn: u16,
    // #[structopt(short = "r", long = "rid", default_value = "42.42.42.42")]
    // rid: Ipv4Addr,
    // #[structopt(short = "t", long = "holdtime", default_value = "42")]
    // hold: u16,
    // #[structopt(short = "l", long = "localip", default_value = "127.0.0.1")]
    // ip: Ipv4Addr,
    // #[structopt(short = "p", long = "port", default_value = "179")]
    // port: u16,
}
async fn connect_to_neighbors(config: Config) {}
async fn start_listener(config: Config) {
    let mut speaker =
        speaker::BGPSpeaker::new(config.asn, u32::from(config.rid), config.holdtime.unwrap());
    let listener = TcpListener::bind(
        config.localip.unwrap().to_string() + ":" + &config.port.unwrap().to_string(),
    )
    .await
    .unwrap();

    loop {
        let (socket, _) = listener.accept().await.unwrap();
        speaker.add_neighbor(socket);
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    // async fn main() -> Result<(), Box<(dyn Error + 'static)>> {
    // async fn main() -> Result<(), Box<(dyn StdError + 'static)>> {
    let opt = Opt::from_args();
    let mut f = std::fs::File::open(&opt.config).unwrap();
    let mut c = String::new();
    f.read_to_string(&mut c).unwrap();
    let mut config: Config = toml::from_str(&c).unwrap();

    config.holdtime = match config.holdtime {
        Some(h) => Some(h),
        None => Some(3),
    };

    config.port = match config.port {
        Some(h) => Some(h),
        None => Some(179),
    };

    config.localip = match config.localip {
        Some(i) => Some(i),
        None => Some("127.0.0.1".parse().unwrap()),
    };

    println!("config: {:?}", config);

    tokio::spawn(start_listener(config)).await.unwrap();
    // let mut speaker = speaker::BGPSpeaker::new(config.asn, u32::from(config.rid), h);
    // let listener = TcpListener::bind(i.to_string() + ":" + &p.to_string())
    //     .await
    //     .unwrap();

    // loop {
    //     let (socket, _) = listener.accept().await?;
    //     speaker.add_neighbor(socket);
    // }
    Ok(())
}
