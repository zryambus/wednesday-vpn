mod cfg;
mod rpc;
mod statistics;
mod storage;
mod wireguard;

use execute::Execute;
use rpc::wireguard::{
    wireguard_control_server, Client, GetStatisticsRequest, GetStatisticsResponse, Server,
    StartWireguardRequest, StartWireguardResponse, StatisticsEntry, SyncConfigRequest,
    SyncConfigResponse,
};
use statistics::ClientEntry;
use tonic::{async_trait, Request, Response, Status};
use std::{path::PathBuf, str::FromStr};

const CONFIG_PATH: &'static str = "/etc/wireguard/wg0.conf";

pub struct WireguardControlServer {}

impl WireguardControlServer {
    fn apply_config(&self, server: &Server, clients: &Vec<Client>) -> Result<(), Status> {
        let cfg = wireguard::config::build_server_config(&server, &clients)?;
        let path = PathBuf::from(CONFIG_PATH);
        std::fs::write(path, cfg)
            .map_err(|e| Status::internal(format!("Failed to write config: {}", e)))?;

        let mut cmd = execute::shell("wg syncconf wg0 <(wg-quick strip wg0)");
        let result = cmd
            .execute()
            .map_err(|e| Status::internal(format!("Failed to sync config: {}", e)))?;

        let exit_code = result.unwrap_or(0);
        if exit_code != 0 {
            return Err(Status::internal(format!(
                "Config sync finished with non-successed exit status: {}",
                exit_code
            )));
        }
        Ok(())
    }

    fn start_wireguard(&self, server: &Server) -> Result<(), Status> {
        let cfg = wireguard::config::build_server_config(&server, &vec![])?;
        let path = PathBuf::from(CONFIG_PATH);
        std::fs::write(path, cfg)
            .map_err(|e| Status::internal(format!("Failed to write config: {}", e)))?;

        let mut cmd = execute::shell("wg-quick up wg0");
        let result = cmd
            .execute()
            .map_err(|e| Status::internal(format!("Failed to up wg0 interface: {}", e)))?;
        let exit_code = result.unwrap_or(0);
        if exit_code != 0 {
            return Err(Status::internal(format!(
                "Wireguard starting finished with non-successed exit status: {}",
                exit_code
            )));
        }
        Ok(())
    }

    fn get_statistics(&self) -> Result<Vec<StatisticsEntry>, Status> {
        let mut cmd = execute::shell("wg show wg0 dump");
        let output = cmd
            .output()
            .map_err(|e| Status::internal(format!("Could not get statistics info: {}", e)))?;
        let data = String::from_utf8(output.stdout)
            .map_err(|e| Status::internal(format!("Could not get string from output: {}", e)))?;
        let mut entries: Vec<StatisticsEntry> = vec![];
        for line in data.lines().skip(1) {
            let entry = ClientEntry::from_str(&line).map_err(|e| {
                Status::internal(format!("Count not convert statistics entry: {}", e))
            })?;
            entries.push(entry.into());
        }
        Ok(entries)
    }
}

#[async_trait]
impl rpc::wireguard::wireguard_control_server::WireguardControl for WireguardControlServer {
    async fn sync_config(
        &self,
        request: Request<SyncConfigRequest>,
    ) -> Result<Response<SyncConfigResponse>, Status> {
        let SyncConfigRequest { server, clients } = request.into_inner();
        let server = server.ok_or(Status::invalid_argument("Field `server` is empty"))?;
        let _ = self.apply_config(&server, &clients)?;
        Ok(Response::new(SyncConfigResponse {}))
    }

    async fn start_wireguard(
        &self,
        request: Request<StartWireguardRequest>,
    ) -> Result<Response<StartWireguardResponse>, Status> {
        let StartWireguardRequest { server } = request.into_inner();
        let server = server.ok_or(Status::invalid_argument("Field `server` is empty"))?;
        let _ = self.start_wireguard(&server)?;
        Ok(Response::new(StartWireguardResponse {}))
    }

    async fn get_statistics(
        &self,
        _request: Request<GetStatisticsRequest>,
    ) -> Result<Response<GetStatisticsResponse>, Status> {
        let entries = self.get_statistics()?;
        Ok(Response::new(GetStatisticsResponse { entries }))
    }
}

use tracing::level_filters::LevelFilter;
use tracing_subscriber::{prelude::*, registry::Registry};

#[tokio::main]
async fn main() -> Result<(), anyhow::Error> {
    let fmt_layer = tracing_subscriber::fmt::layer()
        .with_target(false)
        .with_filter(LevelFilter::DEBUG);

    Registry::default().with(fmt_layer).try_init().unwrap();

    let address = "0.0.0.0:8080".parse().unwrap();
    let wg_control_server = WireguardControlServer {};

    tonic::transport::Server::builder()
        .add_service(wireguard_control_server::WireguardControlServer::new(
            wg_control_server,
        ))
        .serve(address)
        .await?;
    Ok(())
}
