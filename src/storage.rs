use std::sync::Arc;

use anyhow::{anyhow, Result};
use ipnet::IpAdd;
use serde::{Deserialize, Serialize};
use teloxide::prelude::*;
use sqlx::{Pool, postgres::Postgres, FromRow, types::ipnetwork::*, Row};

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
    pub user_id: UserId,
    pub status: UserStatus,
}

impl FromRow<'_, sqlx::postgres::PgRow> for User {
    fn from_row(row: &'_ sqlx::postgres::PgRow) -> std::result::Result<Self, sqlx::Error> {
        Ok(Self{
            user_id: UserId(row.get::<i64, _>("user_id") as u64),
            status: row.get::<UserStatus, _>("status"),
        })
    }
}

#[derive(Clone)]
pub struct Storage {
    pool: Pool<Postgres>,
}

pub type StoragePtr = Arc<Storage>;

impl Storage {
    pub async fn new() -> Result<Self> {
        let pool = Pool::<Postgres>::connect("postgres://wednesday:password@postgres:5432/wednesday_vpn").await?;
        sqlx::migrate!().run(&pool).await?;
        Ok(Self{ pool })
    }

    pub async fn get_profiles(&self) -> Result<Vec<Profile>> {
        let profiles = sqlx::query(r#"
            SELECT * FROM profiles
        "#)
            .fetch_all(&self.pool).await?
            .into_iter().map(|row| FromRow::from_row(&row))
            .flatten().collect();
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
        let profiles = sqlx::query(r#"SELECT * FROM profiles WHERE user_id = $1"#)
            .bind(user_id.0 as i64)
            .fetch_all(&self.pool).await?
            .into_iter().map(|row| FromRow::from_row(&row))
            .flatten().collect();
        Ok(profiles)
    }

    pub async fn get_user_profile(&self, user_id: UserId, name: &String) -> Result<Profile> {
        let row = sqlx::query(r#"SELECT * FROM profiles WHERE user_id = $1 AND name = $2 LIMIT 1"#)
            .bind(user_id.0 as i64)
            .bind(name)
            .fetch_optional(&self.pool).await?;

        if let Some(row) = row {
            Ok(Profile::from_row(&row)?)
        } else {
            Err(anyhow!("Could not find user profile"))
        }
    }

    pub async fn delete_user_profile(&self, user_id: UserId, name: &String) -> Result<()> {
        sqlx::query!("DELETE FROM profiles WHERE user_id = $1 AND name = $2", user_id.0 as i64, name)
            .execute(&self.pool).await?;
        Ok(())
    }

    pub async fn get_user(&self, user_id: UserId) -> Result<User> {
        let row = sqlx::query(r#"SELECT * FROM users WHERE user_id = $1"#)
            .bind(user_id.0 as i64)
            .fetch_optional(&self.pool).await?;
        if row.is_none() {
            let new_user = User {
                user_id,
                status: UserStatus::None,
            };
            sqlx::query(r#"INSERT INTO users (user_id, status) VALUES ($1, $2)"#)
                .bind(new_user.user_id.0 as i64)
                .bind(&new_user.status)
                .execute(&self.pool).await?;
            return Ok(new_user);
        }
        Ok(User::from_row(&row.unwrap())?)
    }

    pub async fn activate_user(&self, user_id: UserId, invite: Invite) -> Result<()> {
        let res = sqlx::query!(r#"DELETE FROM invites WHERE id = $1"#, invite.id)
            .execute(&self.pool).await?;
        if res.rows_affected() == 0 {
            return Err(anyhow!("Invalid invite code"));
        }

        let _user = self.get_user(user_id).await?;

        sqlx::query(r#"UPDATE users SET status = $1 WHERE user_id = $2"#)
            .bind(UserStatus::Granted)
            .bind(user_id.0 as i64)
            .execute(&self.pool).await?;

        Ok(())
    }

    pub async fn get_user_status(&self, user_id: UserId) -> Result<UserStatus> {
        let user = self.get_user(user_id).await?;
        Ok(user.status)
    }

    pub async fn create_invite_code(&self) -> Result<Invite> {
        let invite = Invite::new();
        sqlx::query!(r#"INSERT INTO invites (id) VALUES ($1)"#, invite.id)
            .execute(&self.pool).await?;
        Ok(invite)
    }

    pub async fn revoke_all_invite_codes(&self) -> Result<()> {
        sqlx::query!(r#"DELETE FROM invites"#)
            .execute(&self.pool).await?;
        Ok(())
    }

    pub async fn update_user_status(&self, user_id: UserId, status: UserStatus) -> Result<()> {
        tracing::debug!(
            "Updating user status for user {}, status {:?}",
            user_id,
            status
        );
        sqlx::query(r#"UPDATE users SET status = $1 WHERE user_id = $2"#)
            .bind(status)
            .bind(user_id.0 as i64)
            .execute(&self.pool).await?;
        Ok(())
    }

    pub async fn get_users_with_requested_access(&self) -> Result<Vec<User>> {
        let users = sqlx::query(r#"SELECT * FROM users WHERE status = $1"#)
            .bind(UserStatus::Requested)
            .fetch_all(&self.pool).await?
            .into_iter().map(|row| FromRow::from_row(&row))
            .flatten().collect();
        Ok(users)
    }

    pub async fn get_profile(&self, public_key: &str) -> Result<Profile> {
        let row = sqlx::query(r#"SELECT * FROM profiles WHERE public_key = $1"#)
            .bind(public_key)
            .fetch_one(&self.pool).await?;
        Ok(Profile::from_row(&row)?)
    }
}
