#[derive(Debug)]
pub struct ClientEntry {
    pub pubkey: String,
    pub ip: std::net::Ipv4Addr,
    pub latest_handshake: u64,
    pub tx: u64,
    pub rx: u64,
}

impl std::str::FromStr for ClientEntry {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let entries: Vec<&str> = s.split_ascii_whitespace().collect();
        if entries.len() != 8 {
            return Err(format!("Incorrect input. Entries size should be 8,  not {}", entries.len()));
        }

        let client_entry = ClientEntry {
            pubkey: entries[0].into(),
            ip: entries[3].strip_suffix("/32").unwrap_or("1.2.3.4").parse().map_err(|e| format!("Could not parse ip address: {}", e))?,
            latest_handshake: entries[4].parse().map_err(|e| format!("Could not parse latest_handshake: {}", e))?,
            tx: entries[5].parse().map_err(|e| format!("Could not parse tx: {}", e))?,
            rx: entries[6].parse().map_err(|e| format!("Could not parse rx: {}", e))?
        };

        Ok(client_entry)
    }
}

use std::{fmt, path::PathBuf};

impl fmt::Display for ClientEntry {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> fmt::Result {
        let tx = byte_unit::Byte::from_bytes(self.tx).get_appropriate_unit(true);
        let rx = byte_unit::Byte::from_bytes(self.rx).get_appropriate_unit(true);
        let handshake = std::time::Duration::from_secs(std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH).unwrap().as_secs() - self.latest_handshake);
        let handshake = if self.latest_handshake != 0 {
            humantime::format_duration(handshake).to_string()
        } else {
            "(None)".into()
        };
        write!(f, "Pubkey: {}, IP: {}, Handshake: {}, tx: {}, rx: {}", self.pubkey, self.ip, handshake, tx, rx)
    }
}

mod rpc;
mod wireguard;
mod cfg;
mod storage;

use execute::Execute;
use rpc::wireguard::{SyncConfigRequest, SyncConfigResponse, Server, Client, wireguard_control_server, StartWireguardRequest, StartWireguardResponse};
use tonic::{Request, Response, async_trait, Status};

const CONFIG_PATH: &'static str = "/etc/wireguard/wg0.conf";

pub struct WireguardControlServer{}

impl WireguardControlServer {
    fn apply_config(&self, server: &Server, clients: &Vec<Client>) -> Result<(), Status> {
        let cfg = wireguard::config::build_server_config(&server, &clients)?;
        let path = PathBuf::from(CONFIG_PATH);
        std::fs::write(path, cfg)
            .map_err(|e| Status::internal(format!("Failed to write config: {}", e)))?;

        let mut cmd = execute::shell("wg syncconf wg0 <(wg-quick strip wg0)");
        let result = cmd.execute()
            .map_err(|e| Status::internal(format!("Failed to sync config: {}", e)))?;

        let exit_code = result.unwrap_or(0);
        if exit_code != 0 {
            return Err(Status::internal(format!("Config sync finished with non-successed exit status: {}", exit_code)));
        }
        Ok(())
    }

    fn start_wireguard(&self, server: &Server) -> Result<(), Status> {
        let cfg = wireguard::config::build_server_config(&server, &vec![])?;
        let path = PathBuf::from(CONFIG_PATH);
        std::fs::write(path, cfg)
            .map_err(|e| Status::internal(format!("Failed to write config: {}", e)))?;

        let mut cmd = execute::shell("wg-quick up wg0");
        let result = cmd.execute()
            .map_err(|e| Status::internal(format!("Failed to up wg0 interface: {}", e)))?;
        let exit_code = result.unwrap_or(0);
        if exit_code != 0 {
            return Err(Status::internal(format!("Wireguard starting finished with non-successed exit status: {}", exit_code)));
        }
        Ok(())
    }
}

#[async_trait]
impl rpc::wireguard::wireguard_control_server::WireguardControl for WireguardControlServer {
    async fn sync_config(&self, request: Request<SyncConfigRequest>) -> Result<Response<SyncConfigResponse>, Status> {
        let SyncConfigRequest{ server, clients } = request.into_inner();
        let server = server.ok_or(Status::invalid_argument("Field `server` is empty"))?;
        let _ = self.apply_config(&server, &clients)?;
        Ok(Response::new(SyncConfigResponse{}))
    }

    async fn start_wireguard(&self, request: Request<StartWireguardRequest>) -> Result<Response<StartWireguardResponse>, Status> {
        let StartWireguardRequest { server } = request.into_inner();
        let server = server.ok_or(Status::invalid_argument("Field `server` is empty"))?;
        let _ = self.start_wireguard(&server)?;        
        Ok(Response::new(StartWireguardResponse{}))
    }
}

use tracing::level_filters::LevelFilter;
use tracing_subscriber::{prelude::*, registry::Registry};

#[tokio::main]
async fn main() -> Result<(), anyhow::Error> {
    let fmt_layer = tracing_subscriber::fmt::layer()
        .with_target(false)
        .with_filter(LevelFilter::DEBUG);

    Registry::default()
        .with(fmt_layer)
        .try_init()
        .unwrap();
        
    let address = "0.0.0.0:8080".parse().unwrap();
    let wg_control_server = WireguardControlServer{};

    tonic::transport::Server::builder()
        .add_service(wireguard_control_server::WireguardControlServer::new(wg_control_server))
        .serve(address)
        .await?;
    Ok(())
}
