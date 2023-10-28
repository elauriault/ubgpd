use tokio::time::{sleep, Duration};
// use serde::Deserialize;
use async_std::sync::{Arc, Mutex};
use std::error::Error;
// use std::fs::File;
use clap::Parser;
use std::io::prelude::*;

#[macro_use]
extern crate derive_builder;

mod bgp;
mod config;
mod fib;
mod neighbor;
mod rib;
mod speaker;

#[derive(Debug, clap::StructOpt)]
#[structopt(name = "ubgpd", about = "A minimalistic bgp daemon written in rust.")]
struct Opt {
    #[structopt(short = 'c', long = "config", default_value = "ubgpd.conf")]
    config: String,
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    let opt = Opt::parse();
    let mut f = std::fs::File::open(&opt.config).unwrap();
    let mut c = String::new();
    f.read_to_string(&mut c).unwrap();
    let mut config: config::Config = toml::from_str(&c).unwrap();

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

    let speaker = Arc::new(Mutex::new(speaker::BGPSpeaker::new(
        config.asn,
        u32::from(config.rid),
        config.holdtime.unwrap(),
        config.localip.unwrap(),
        config.port.unwrap(),
    )));

    {
        let neighbors = config.neighbors.unwrap();
        // let speaker = speaker.clone();
        let mut speaker = speaker.lock().await;
        for n in neighbors {
            n.families = match n.families {
                Some(i) => Some(i),
                None => {
                    let a = bgp::AddressFamily {
                        afi: bgp::AFI::Ipv4,
                        safi: bgp::SAFI::NLRIUnicast,
                    };
                    Some(vec![a])
                }
            };
            speaker.add_neighbor(n, None).await;
        }
    }

    tokio::spawn(async move { speaker::BGPSpeaker::start(speaker).await });

    loop {
        sleep(Duration::from_secs(1)).await;
    }
}
