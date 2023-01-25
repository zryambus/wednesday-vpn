use crate::rpc::wireguard::StatisticsEntry;
use std::fmt;

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
            return Err(format!(
                "Incorrect input. Entries size should be 8,  not {}",
                entries.len()
            ));
        }

        let client_entry = ClientEntry {
            pubkey: entries[0].into(),
            ip: entries[3]
                .strip_suffix("/32")
                .unwrap_or("1.2.3.4")
                .parse()
                .map_err(|e| format!("Could not parse ip address: {}", e))?,
            latest_handshake: entries[4]
                .parse()
                .map_err(|e| format!("Could not parse latest_handshake: {}", e))?,
            tx: entries[5]
                .parse()
                .map_err(|e| format!("Could not parse tx: {}", e))?,
            rx: entries[6]
                .parse()
                .map_err(|e| format!("Could not parse rx: {}", e))?,
        };

        Ok(client_entry)
    }
}

impl std::convert::Into<StatisticsEntry> for ClientEntry {
    fn into(self) -> StatisticsEntry {
        StatisticsEntry {
            public_key: self.pubkey,
            ip: self.ip.into(),
            latest_handshake: self.latest_handshake,
            tx: self.tx,
            rx: self.rx,
        }
    }
}

impl std::convert::From<StatisticsEntry> for ClientEntry {
    fn from(value: StatisticsEntry) -> Self {
        Self {
            pubkey: value.public_key,
            ip: std::net::Ipv4Addr::from(value.ip),
            latest_handshake: value.latest_handshake,
            tx: value.tx,
            rx: value.rx,
        }
    }
}

impl fmt::Display for ClientEntry {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let tx = byte_unit::Byte::from_bytes(self.tx).get_appropriate_unit(true);
        let rx = byte_unit::Byte::from_bytes(self.rx).get_appropriate_unit(true);
        let handshake = std::time::Duration::from_secs(
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_secs()
                - self.latest_handshake,
        );
        let handshake = if self.latest_handshake != 0 {
            humantime::format_duration(handshake).to_string()
        } else {
            "(None)".into()
        };
        write!(
            f,
            "IP: {}, handshake: {}, tx: {}, rx: {}",
            self.ip, handshake, tx, rx
        )
    }
}
