use std::{sync::Arc};

use mongodb::{Client, Database, bson::doc, Collection, options::ClientOptions};
use anyhow::{Result, anyhow, Ok};
use serde::{Serialize, Deserialize};
use teloxide::prelude::*;
use futures::stream::TryStreamExt;
use ipnet::IpAdd;

use crate::{cfg::CfgPtr, wireguard::{config::{build_peer_config, PeerConfig}, keys::gen_keys}};

#[derive(Serialize, Deserialize, Debug)]
pub struct WGProfile {
    pub name: String,
    pub user_id: String,

    pub ip: std::net::Ipv4Addr,
    
    pub private_key: String,
    pub public_key: String,

    pub enabled: bool,
    pub only_local: bool 
}

#[derive(Serialize, Deserialize, Debug)]
pub struct Invite {
    pub id: String,
}

impl Invite {
    pub fn new() -> Self {
        let id = uuid::Uuid::new_v4().to_string();
        Self { id }   
    }
}

#[derive(Serialize, Deserialize, Debug)]
pub struct User {
    pub user_id: String,
    pub active: bool,
}

#[derive(Clone)]
pub struct Storage {
    client: Client,
    database: Database,
}

pub type StoragePtr = Arc<Storage>;

impl Storage {
    pub async fn new() -> Result<Self> {
        let options = ClientOptions::parse("mongodb://mongo:27017").await?;
        let client = Client::with_options(options)?;
        let database = client.database("wednesday");
        Ok(Self {
            client,
            database
        })
    }

    fn profiles_collection(&self) -> Collection<WGProfile> {
        self.database.collection::<WGProfile>("clients")
    }

    fn invites_collection(&self) -> Collection<Invite> {
        self.database.collection("invites")
    }

    fn users_collection(&self) -> Collection<User> {
        self.database.collection("users")
    }

    pub async fn get_clients(&self) -> Result<Vec<WGProfile>> {
        let filter = doc!{};
        let clients: Vec<WGProfile> = self.profiles_collection()
            .find(filter, None).await?
            .try_collect().await?;
        Ok(clients)
    }

    pub async fn add_profile(&self, name: &String, user_id: UserId) -> Result<()> {
        let filter = doc! { "name": name.clone(), "user_id": user_id.to_string() };
        let existed = self.profiles_collection().find_one(Some(filter), None).await?;
        if let Some(_) = existed {
            return Err(anyhow!("Profile with name '{}' already existing", name));
        }
        let max = self.get_clients().await?.into_iter()
            .map(|c| c.ip)
            .max();
 
        let gateway = std::net::Ipv4Addr::new(10, 9, 0, 1);

        let max = if let Some(value) = max {
            value
        } else {
            gateway
        };

        let ip = std::net::Ipv4Addr::from(max).saturating_add(1);

        let (private, public) = gen_keys()?;
                
        let profile = WGProfile{
            enabled: false,
            ip: ip,
            only_local: false,
            name: name.clone(),
            private_key: private.to_owned(),
            public_key: public.to_owned(),
            user_id: user_id.to_string()
        };
        self.profiles_collection().insert_one(&profile, None).await?;
        Ok(())
    }

    pub async fn get_user_profiles(&self, user_id: UserId) -> Result<Vec<WGProfile>> {
        let filter = doc! { "user_id": user_id.to_string() };
        let profiles: Vec<WGProfile> = self.profiles_collection()
            .find(filter, None).await?
            .try_collect().await?;
        Ok(profiles)
    }

    pub async fn get_user_profile(&self, user_id: UserId, name: &String) -> Result<WGProfile> {
        let filter = doc!{ "user_id": user_id.to_string(), "name": name };
        let profile = self.profiles_collection().find_one(filter, None).await?;
        if let Some(profile) = profile {
            Ok(profile)
        } else {
            Err(anyhow!("Could not find user profile"))
        }
    }

    pub async fn update_user_profile(&self, user_id: UserId, name: &String, profile: WGProfile) -> Result<()> {
        let filter = doc!{ "user_id": user_id.to_string(), "name": name };
        self.profiles_collection().find_one_and_replace(filter, profile, None).await?;
        Ok(())
    }

    pub async fn delete_user_profile(&self, user_id: UserId, name: &String) -> Result<()> {
        let filter = doc!{ "user_id": user_id.to_string(), "name": name };
        self.profiles_collection().find_one_and_delete(filter, None).await?;
        Ok(())
    }

    pub async fn get_user(&self, user_id: UserId) -> Result<Option<User>> {
        let filter = doc!{ "user_id": user_id.to_string() };
        let user = self.users_collection().find_one(filter, None).await?;
        Ok(user)
    }

    pub async fn activate_user(&self, user_id: UserId, invite: Invite) -> Result<()> {
        let filter = doc!{ "id": invite.id };
        let res = self.invites_collection().find_one_and_delete(filter, None).await?;
        if res.is_none() {
            return Err(anyhow!("Invalid invite code"));
        }

        let user = User{
            user_id: user_id.to_string(),
            active: true,
        };

        self.users_collection().insert_one(&user, None).await?;

        Ok(())
    }

    pub async fn is_active_user(&self, user_id: UserId) -> Result<bool> {
        let user = self.get_user(user_id).await?;
        if user.is_some() && user.unwrap().active {
            return Ok(true);
        }
        Ok(false)
    }

    pub async fn create_invite_code(&self) -> Result<Invite> {
        let invite = Invite::new();
        self.invites_collection().insert_one(&invite, None).await?;
        Ok(invite)
    }

    pub async fn revoke_all_invite_codes(&self) -> Result<()> {
        self.invites_collection().delete_many(doc!{}, None).await?;
        Ok(())
    }
}
