use clap::Parser;
use std::path::PathBuf;
use std::sync::Arc;
use std::{collections::HashMap, error::Error};
use tokio::sync::Mutex;
use tokio::time::{sleep, Duration};

#[macro_use]
extern crate derive_builder;

mod bgp;
mod config;
mod fib;
mod grpc;
mod neighbor;
mod rib;
mod speaker;

#[derive(Parser)]
#[command(author, version, about, long_about = None)]
struct Opt {
    #[arg(short, long, value_parser, default_value = "ubgpd.conf")]
    config: PathBuf,
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    env_logger::init();
    let opt = Opt::parse();
    let mut config = config::read_config(&opt.config)?;

    config.hold_time = match config.hold_time {
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

    config.families = match config.families {
        Some(i) => Some(i),
        None => {
            let a = bgp::AddressFamily {
                afi: bgp::Afi::Ipv4,
                safi: bgp::Safi::NLRIUnicast,
            };
            Some(vec![a])
        }
    };

    let families = config.families.clone();
    let speaker = Arc::new(Mutex::new(speaker::BGPSpeaker::new(
        config.asn,
        u32::from(config.rid),
        config.hold_time.unwrap(),
        config.localip.unwrap(),
        config.port.unwrap(),
        families.unwrap(),
    )));

    {
        let neighbors = config.neighbors.unwrap();
        let mut speaker = speaker.lock().await;
        for mut n in neighbors {
            let families = config.families.clone();
            n.families = match n.families {
                Some(i) => Some(i),
                None => families.clone(),
            };
            n.hold_time = match n.hold_time {
                Some(i) => Some(i),
                None => Some(speaker.hold_time),
            };
            speaker.add_neighbor(n, HashMap::new()).await;
        }
    }

    let s1 = speaker.clone();

    tokio::spawn(async move { speaker::BGPSpeaker::start(speaker).await });
    tokio::spawn(async move { grpc::grpc_server(s1).await });

    loop {
        sleep(Duration::from_secs(1)).await;
    }
}
