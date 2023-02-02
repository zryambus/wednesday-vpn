use std::sync::Arc;

use anyhow::{anyhow, Result};
use futures::{stream::TryStreamExt, StreamExt};
use ipnet::IpAdd;
// use mongodb::{bson::doc, options::ClientOptions, Client, Collection, Database};
use serde::{Deserialize, Serialize};
use serde_repr::{Deserialize_repr, Serialize_repr};
use teloxide::prelude::*;
// use bb8::{Builder, Pool};
// use bb8_postgres::{PostgresConnectionManager, tokio_postgres::{config::Config, NoTls, GenericClient}};
use std::str::FromStr;
use sqlx::{Pool, postgres::Postgres, query, FromRow, types::ipnetwork::*, Row};

use crate::{
    cfg::CfgPtr,
    wireguard::{
        config::{build_peer_config, PeerConfig},
        keys::gen_keys,
    },
};

#[derive(Serialize, Deserialize, Debug)]
pub struct Profile {
    pub name: String,
    pub user_id: UserId,

    pub ip: std::net::IpAddr,

    pub private_key: String,
    pub public_key: String,

    pub only_local: bool,
}

impl sqlx::FromRow<'_, sqlx::postgres::PgRow> for Profile {
    fn from_row(row: &'_ sqlx::postgres::PgRow) -> std::result::Result<Self, sqlx::Error> {
        Ok(Self{
            name: row.get("name"),
            user_id: UserId(row.get::<i64, _>("user_id") as u64),
            ip: row.get::<IpNetwork, _>("ip").ip(),
            private_key: row.get("private_key"),
            public_key: row.get("public_key"),
            only_local: row.get("only_local"),
        })
    }
}

#[derive(Debug)]
pub struct Invite {
    pub id: uuid::Uuid,
}

impl Invite {
    pub fn new() -> Self {
        let id = uuid::Uuid::new_v4();
        Self { id }
    }
}

#[derive(Default, Debug, sqlx::Type)]
#[sqlx(type_name = "user_status")]
#[sqlx(rename_all = "lowercase")]
pub enum UserStatus {
    #[default]
    None,
    Requested,
    Granted,
    Restricted,
}

#[derive(Debug)]
pub struct User {
    pub user_id: String,
    pub status: UserStatus,
}

#[derive(Clone)]
pub struct Storage {
    pool: Pool<Postgres>,
}

pub type StoragePtr = Arc<Storage>;

impl Storage {
    pub async fn new() -> Result<Self> {
        let pool = Pool::<Postgres>::connect("postgres://wednesday:password@postgres:5432").await?;
        sqlx::migrate!().run(&pool).await?;
        Ok(Self{ pool })
    }

    pub async fn get_profiles(&self) -> Result<Vec<Profile>> {
        let profiles = sqlx::query_as!(Profile, r#"
            SELECT * FROM profiles
        "#).fetch_all(&self.pool).await?;
        // let profiles = rows.into_iter().map(|row| Some(Profile{
        //     name: row.name,
        //     user_id: UserId(row.user_id as u64),
        //     ip: row.ip.ip(),
        //     private_key: row.private_key,
        //     public_key: row.public_key,
        //     only_local: row.only_local,
        // })).flatten().collect();
        Ok(profiles)
    }

    pub async fn add_profile(&self, name: &String, user_id: UserId) -> Result<()> {
        let exists = sqlx::query!(
            r#"SELECT * FROM profiles WHERE name = $1 AND user_id = $2"#,
            name, user_id.0 as i64
        )
            .fetch_optional(&self.pool).await?
            .is_some();

        if exists {
            return Err(anyhow!("Profile with name '{}' already existing", name));
        }

        let max = self.get_profiles().await?.into_iter().map(|c| c.ip).max();

        let gateway = std::net::Ipv4Addr::new(10, 9, 0, 1);

        let max = if let Some(std::net::IpAddr::V4(value)) = max {
            value
        } else {
            gateway
        };

        let ip = std::net::Ipv4Addr::from(max).saturating_add(1);

        let (private, public) = gen_keys()?;

        let profile = Profile {
            ip: ip.into(),
            only_local: false,
            name: name.clone(),
            private_key: private.to_owned(),
            public_key: public.to_owned(),
            user_id,
        };
        sqlx::query!(
            r#"INSERT INTO profiles (name, user_id, ip, private_key, public_key, only_local) VALUES ($1, $2, $3, $4, $5, $6)"#,
            profile.name, profile.user_id.0 as i64, IpNetwork::from(profile.ip), profile.private_key, profile.public_key, profile.only_local
        ).execute(&self.pool).await?;
        Ok(())
    }

    pub async fn get_user_profiles(&self, user_id: UserId) -> Result<Vec<Profile>> {
        let filter = doc! { "user_id": user_id.to_string() };
        let profiles: Vec<Profile> = self
            .profiles_collection()
            .find(filter, None)
            .await?
            .try_collect()
            .await?;
        Ok(profiles)
    }

    pub async fn get_user_profile(&self, user_id: UserId, name: &String) -> Result<Profile> {
        let filter = doc! { "user_id": user_id.to_string(), "name": name };
        let profile = self.profiles_collection().find_one(filter, None).await?;
        if let Some(profile) = profile {
            Ok(profile)
        } else {
            Err(anyhow!("Could not find user profile"))
        }
    }

    pub async fn update_user_profile(
        &self,
        user_id: UserId,
        name: &String,
        profile: Profile,
    ) -> Result<()> {
        let filter = doc! { "user_id": user_id.to_string(), "name": name };
        self.profiles_collection()
            .find_one_and_replace(filter, profile, None)
            .await?;
        Ok(())
    }

    pub async fn delete_user_profile(&self, user_id: UserId, name: &String) -> Result<()> {
        let filter = doc! { "user_id": user_id.to_string(), "name": name };
        self.profiles_collection()
            .find_one_and_delete(filter, None)
            .await?;
        Ok(())
    }

    pub async fn get_user(&self, user_id: UserId) -> Result<User> {
        let filter = doc! { "user_id": user_id.to_string() };
        let user = self.users_collection().find_one(filter, None).await?;
        if user.is_none() {
            let new_user = User {
                user_id: user_id.to_string(),
                status: UserStatus::None,
            };
            self.users_collection().insert_one(&new_user, None).await?;
            return Ok(new_user);
        }
        Ok(user.unwrap())
    }

    pub async fn activate_user(&self, user_id: UserId, invite: Invite) -> Result<()> {
        let filter = doc! { "id": invite.id };
        let res = self
            .invites_collection()
            .find_one_and_delete(filter, None)
            .await?;
        if res.is_none() {
            return Err(anyhow!("Invalid invite code"));
        }

        let _user = self.get_user(user_id).await?;

        let query = doc! { "user_id": user_id.to_string() };
        let update = doc! { "$set": { "status": UserStatus::Granted as u32 } };
        self.users_collection()
            .update_one(query, update, None)
            .await?;

        Ok(())
    }

    pub async fn get_user_status(&self, user_id: UserId) -> Result<UserStatus> {
        let user = self.get_user(user_id).await?;
        Ok(user.status)
    }

    pub async fn create_invite_code(&self) -> Result<Invite> {
        let invite = Invite::new();
        self.invites_collection().insert_one(&invite, None).await?;
        Ok(invite)
    }

    pub async fn revoke_all_invite_codes(&self) -> Result<()> {
        self.invites_collection().delete_many(doc! {}, None).await?;
        Ok(())
    }

    pub async fn update_user_status(&self, user_id: UserId, status: UserStatus) -> Result<()> {
        tracing::debug!(
            "Updating user status for user {}, status {:?}",
            user_id,
            status
        );
        let query = doc! { "user_id": user_id.to_string() };
        let update = doc! { "$set": { "status": status as u32 } };
        self.users_collection()
            .update_one(query, update, None)
            .await?;
        Ok(())
    }

    pub async fn get_users_with_requested_access(&self) -> Result<Vec<User>> {
        let filter = doc! { "status": UserStatus::Requested as u32 };
        let users = self
            .users_collection()
            .find(filter, None)
            .await?
            .try_collect()
            .await?;
        Ok(users)
    }

    pub async fn get_profile(&self, public_key: &str) -> Result<Profile> {
        let filter = doc!{ "public_key": public_key };
        let profile = self.profiles_collection().find_one(filter, None).await?.ok_or(anyhow!("Could not find profile in database"))?;
        Ok(profile)
    }
}
