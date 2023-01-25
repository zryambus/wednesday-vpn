use crate::{
    cfg::CfgPtr,
    rpc::wireguard::{Client, Server},
    storage::WGProfile,
};
use serde::Serialize;
use tinytemplate::TinyTemplate;
use tonic::{Code, Status};

#[derive(Serialize)]
struct ServerTemplateCtx {
    server_ip: String,
    server_subnet: u8,
    port: u16,
    server_private_key: String,
    post_up: String,
    pre_down: String,
}

const SERVER_TEMPLATE: &str = "[Interface]
Address = {server_ip}/{server_subnet}
ListenPort = {port}
PrivateKey = {server_private_key}
MTU = 1450
PostUp = {post_up}
PreDown = {pre_down}";

#[derive(Serialize)]
struct PeerCtx {
    client_public_key: String,
    client_ip: String,
}

const PEER_TEMPLATE: &str = "[Peer]
PublicKey = {client_public_key}
AllowedIPs = {client_ip}/32";

#[derive(Serialize)]
struct PeerConfigCtx {
    peer_private_key: String,
    peer_ip: String,
    server_public_key: String,
    endpoint: String,
    port: u16,
    only_local: bool,
    dns: String,
}

const PEER_CONFIG_TEMPLATE: &str = "[Interface]
PrivateKey = {peer_private_key}
Address = {peer_ip}/32
DNS = {dns}

[Peer]
PublicKey = {server_public_key}
AllowedIPs = {{if not only_local }}0.0.0.0/0{{ else }}10.9.0.0/24{{ endif }}
Endpoint = {endpoint}:{port}";

pub fn build_server_config(server: &Server, clients: &Vec<Client>) -> Result<String, Status> {
    let mut config = String::new();

    let mut tt = TinyTemplate::new();
    tt.add_template("server_template", SERVER_TEMPLATE)
        .map_err(|e| Status::new(Code::Internal, format!("{}", e)))?;
    tt.add_template("peer_template", PEER_TEMPLATE)
        .map_err(|e| Status::new(Code::Internal, format!("{}", e)))?;

    let ctx = ServerTemplateCtx {
        server_private_key: server.key.clone(),
        port: server.port as u16,
        server_ip: std::net::Ipv4Addr::from(server.ip).to_string(),
        server_subnet: server.subnet as u8,
        post_up: server.post_up.clone(),
        pre_down: server.pre_down.clone(),
    };
    config.push_str(
        &tt.render("server_template", &ctx)
            .map_err(|e| Status::new(Code::Internal, format!("{}", e)))?,
    );
    config.push_str("\n\n");

    for client in clients {
        let ctx = PeerCtx {
            client_ip: std::net::Ipv4Addr::from(client.ip).to_string(),
            client_public_key: client.key.clone(),
        };

        config.push_str(
            &tt.render("peer_template", &ctx)
                .map_err(|e| Status::new(Code::Internal, format!("{}", e)))?,
        );
        config.push_str("\n\n");
    }

    Ok(config)
}

pub struct PeerConfig {
    endpoint: String,
    key: String,
    ip: u32,
    port: u16,
    dns: Vec<u32>,
    public_key: String,
}

impl PeerConfig {
    pub fn new(profile: &WGProfile, cfg: &CfgPtr) -> anyhow::Result<Self> {
        let endpoint_is_ip = cfg.endpoint.parse::<std::net::Ipv4Addr>().is_ok();
        let endpoint_is_domain = url::Url::parse(&cfg.endpoint).is_ok();
        if !endpoint_is_ip && !endpoint_is_domain {
            return Err(anyhow::anyhow!("Invalid endpoint"));
        }

        Ok(Self {
            ip: profile.ip.into(),
            key: profile.private_key.clone(),
            endpoint: cfg.endpoint.clone(),
            port: cfg.port,
            dns: vec![std::net::Ipv4Addr::new(8, 8, 8, 8).into()],
            public_key: cfg.public_key.clone(),
        })
    }
}

pub fn build_peer_config(peer_cfg: &PeerConfig) -> Result<String, tinytemplate::error::Error> {
    let mut tt = tinytemplate::TinyTemplate::new();
    tt.add_template("peer_config_template", PEER_CONFIG_TEMPLATE)?;
    let ctx = PeerConfigCtx {
        peer_ip: std::net::Ipv4Addr::from(peer_cfg.ip).to_string(),
        peer_private_key: peer_cfg.key.clone(),
        server_public_key: peer_cfg.public_key.clone(),
        endpoint: peer_cfg.endpoint.clone(),
        only_local: false,
        port: peer_cfg.port,
        dns: peer_cfg
            .dns
            .clone()
            .into_iter()
            .map(|ip| std::net::Ipv4Addr::from(ip).to_string())
            .collect::<Vec<String>>()
            .join(", "),
    };
    let config = tt.render("peer_config_template", &ctx)?;
    Ok(config)
}

// PostUp = iptables -A FORWARD -i %i -j ACCEPT; iptables -t nat -A POSTROUTING -o eth0 -j MASQUERADE
// PostDown = iptables -D FORWARD -i %i -j ACCEPT; iptables -t nat -D POSTROUTING -o eth0 -j MASQUERADE

#[test]
fn test_build_server_config() {
    let server = Server {
        key: "YE3x5BL8N36oPZ9N2HbQIrPPGI+b+Qk86TjrU+FJonU=".into(),
        ip: std::net::Ipv4Addr::new(10, 9, 0, 1).into(),
        port: 51820,
        subnet: 24,
        dns: vec![std::net::Ipv4Addr::new(8, 8, 8, 8).into()],
        post_up: "iptables -t nat -I POSTROUTING -o eth0 -j MASQUERADE".into(),
        pre_down: "iptables -t nat -D POSTROUTING -o eth0 -j MASQUERADE".into(),
    };

    let clients: Vec<Client> = vec![Client {
        ip: std::net::Ipv4Addr::new(10, 9, 0, 2).into(),
        key: "qMzUy9H0ISe8AMNIs2Pm+RmVYdUxn9b3XfEAOILTfVA=".into(),
    }];

    let res = build_server_config(&server, &clients).expect("Could not build server config");
    println!("{}", res);
}

#[test]
fn test_build_peer_config() {
    let cfg = PeerConfig {
        ip: std::net::Ipv4Addr::new(10, 9, 0, 2).into(),
        key: "GGEjcrm6GXFlunqnT0HY23jWqaQ402C371jfblVaw3w=".into(),
        endpoint: "127.0.0.1".into(),
        port: 51820,
        dns: vec![
            std::net::Ipv4Addr::new(8, 8, 8, 8).into(),
            std::net::Ipv4Addr::new(1, 1, 1, 1).into(),
        ],
        public_key: "vvM86VntTg9J4XhtDd3tRN0XS6zUS+6OgiwTmx+FeEk=".into(),
    };

    let res = build_peer_config(&cfg).expect("Could not build peer config");
    println!("{}", res);
}
