use crate::{
    rpc::wireguard::{
        wireguard_control_client::WireguardControlClient, SyncConfigRequest, Client, Server, StartWireguardRequest,
        GetStatisticsRequest, GetStatisticsResponse,
    },
    storage::StoragePtr,
    cfg::CfgPtr,
    statistics::ClientEntry,
};

use anyhow::Result;

pub async fn sync_config(storage: &StoragePtr, cfg: &CfgPtr) -> Result<()> {
    let mut client = WireguardControlClient::connect("http://wgc:8080").await?;
    let request = SyncConfigRequest{
        server: Some(Server {
            key: cfg.private_key.clone(),
            ip: std::net::Ipv4Addr::new(10, 9, 0, 1).into(),
            port: 51820,
            subnet: 24,
            dns: vec![std::net::Ipv4Addr::new(8, 8, 8, 8).into()],
            post_up: cfg.post_up.clone(),
            pre_down: cfg.pre_down.clone(),
        }),
        clients: storage.get_profiles().await?.into_iter()
            .map(|c| Some(Client{
                ip: if let std::net::IpAddr::V4(ip) = c.ip { ip.into() } else { return None },
                key: c.public_key
            }))
            .flatten()
            .collect()
    };
    let _response = client.sync_config(request).await?;
    Ok(())
}


pub async fn start_wireguard_server(cfg: &CfgPtr) -> Result<()> {
    let mut client = WireguardControlClient::connect("http://wgc:8080").await?;
    let request = StartWireguardRequest{
        server: Some(Server {
            key: cfg.private_key.clone(),
            ip: std::net::Ipv4Addr::new(10, 9, 0, 1).into(),
            port: 51820,
            subnet: 24,
            dns: vec![std::net::Ipv4Addr::new(8, 8, 8, 8).into()],
            post_up: cfg.post_up.clone(),
            pre_down: cfg.pre_down.clone(),
        }),
    };
    let _ = client.start_wireguard(request).await?;
    Ok(())
}

pub async fn get_statistics() -> Result<Vec<ClientEntry>> {
    let mut client = WireguardControlClient::connect("http://wgc:8080").await?;
    let request = GetStatisticsRequest{};
    let response = client.get_statistics(request).await?; 
    let GetStatisticsResponse{ entries } = response.into_inner();
    let entries: Vec<ClientEntry> = entries.into_iter().map(|e| e.into()).collect();
    Ok(entries)
}
