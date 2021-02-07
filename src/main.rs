use std::error::Error;
use std::net::Ipv4Addr;
use structopt::StructOpt;
use tokio::net::TcpListener;

#[macro_use]
extern crate derive_builder;

mod bgp;
mod speaker;

#[derive(Debug, StructOpt)]
#[structopt(name = "ubgpd", about = "A minimalistic bgp daemon written in rust.")]
struct Opt {
    #[structopt(short = "a", long = "asn", default_value = "42")]
    asn: u16,
    #[structopt(short = "r", long = "rid", default_value = "42.42.42.42")]
    rid: Ipv4Addr,
    #[structopt(short = "t", long = "holdtime", default_value = "42")]
    hold: u16,
    #[structopt(short = "l", long = "localip", default_value = "127.0.0.1")]
    ip: Ipv4Addr,
    #[structopt(short = "p", long = "port", default_value = "179")]
    port: u16,
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    let opt = Opt::from_args();
    let mut speaker = speaker::BGPSpeaker::new(opt.asn, u32::from(opt.rid), opt.hold);
    let listener = TcpListener::bind(opt.ip.to_string() + ":" + &opt.port.to_string()).await?;

    loop {
        let (socket, _) = listener.accept().await?;
        speaker.add_neighbor(socket);
    }
}
