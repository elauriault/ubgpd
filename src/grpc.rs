use ipnet::IpNet;
use num_traits::FromPrimitive;
use std::net::IpAddr;

use async_std::sync::{Arc, Mutex};
use tonic::{transport::Server, Request, Response, Status};

use ubgp::config_server::{Config, ConfigServer};
use ubgp::state_server::{State, StateServer};
use ubgp::{NeighborEntry, NeighborReply, NeighborRequest, RibEntry, RibReply, RibRequest};

pub mod ubgp {
    tonic::include_proto!("ubgp");
}

use crate::bgp;
use crate::speaker;

#[derive(Debug)]
pub struct GrpcServer {
    speaker: Arc<Mutex<speaker::BGPSpeaker>>,
}

impl GrpcServer {
    pub fn new(speaker: Arc<Mutex<speaker::BGPSpeaker>>) -> Self {
        GrpcServer { speaker }
    }
}

#[tonic::async_trait]
impl Config for GrpcServer {
    async fn get_neighbor_config(
        &self,
        request: Request<NeighborRequest>,
    ) -> Result<Response<NeighborReply>, Status> {
        println!("Got a neighbor request: {:?}", request);

        let mut entries = vec![];
        let neighbors;
        {
            let s = self.speaker.lock().await;
            neighbors = s.neighbors.clone();
        }

        match request.into_inner().ip {
            None => {
                for n in neighbors {
                    let n = n.lock().await;
                    let entry = NeighborEntry {
                        ip: n.remote_ip.unwrap().to_string(),
                        port: n.remote_port.unwrap() as u32,
                        asn: n.remote_asn.unwrap() as u32,
                        routerid: n.remote_rid.unwrap(),
                    };
                    entries.push(entry);
                }
            }
            Some(ip) => {
                let ip: IpAddr = ip.parse().unwrap();
                for n in neighbors {
                    let n = n.lock().await;
                    if ip == n.remote_ip.unwrap() {
                        let entry = NeighborEntry {
                            ip: n.remote_ip.unwrap().to_string(),
                            port: n.remote_port.unwrap() as u32,
                            asn: n.remote_asn.unwrap() as u32,
                            routerid: n.remote_rid.unwrap(),
                        };
                        entries.push(entry);
                    }
                }
            }
        }

        let reply = ubgp::NeighborReply { neighbors: entries };

        Ok(Response::new(reply))
    }
}

#[tonic::async_trait]
impl State for GrpcServer {
    async fn get_rib(&self, request: Request<RibRequest>) -> Result<Response<RibReply>, Status> {
        println!("Got a rib request: {:?}", request);

        let mut entries = vec![];

        let afi = request.get_ref().afi as u16;
        let afi: bgp::Afi = FromPrimitive::from_u16(afi).unwrap();
        let safi = request.get_ref().safi as u8;
        let safi: bgp::Safi = FromPrimitive::from_u8(safi).unwrap();

        let af = bgp::AddressFamily { afi, safi };

        let rib;
        {
            let s = self.speaker.lock().await;
            rib = s.rib.get(&af);
            match rib {
                None => {}
                Some(rib) => {
                    let rib = rib.lock().await;
                    for (n, _a) in rib.iter() {
                        let n: IpNet = n.into();
                        let nlri = n.to_string();
                        let entry = RibEntry { nlri };
                        entries.push(entry);
                    }
                }
            }
        }

        let reply = ubgp::RibReply { nlris: entries };

        Ok(Response::new(reply))
    }
}

pub async fn grpc_server(speaker: Arc<Mutex<speaker::BGPSpeaker>>) {
    let addr = "127.0.0.1:50051".parse().unwrap();
    let config_server = GrpcServer::new(speaker.clone());
    let state_server = GrpcServer::new(speaker);

    Server::builder()
        .add_service(ConfigServer::new(config_server))
        .add_service(StateServer::new(state_server))
        .serve(addr)
        .await
        .unwrap();
}
