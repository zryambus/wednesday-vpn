use std::{
    io::Write,
    process::{Command, Stdio},
    sync::Arc,
};

use anyhow::{anyhow, Result};
use rand::Rng;
use serde::{Deserialize, Serialize};
use teloxide::{
    dispatching::dialogue::{InMemStorage, Storage},
    macros::BotCommands,
    prelude::*,
    types::{ChatId, InlineKeyboardButton, InlineKeyboardMarkup, InputFile, ParseMode},
};

use super::AddProfileDialogueState;
use crate::{
    cfg::CfgPtr,
    control_client::sync_config,
    storage::{Invite, StoragePtr, UserStatus},
    wireguard::config::{build_peer_config, PeerConfig},
};

#[derive(Default, Clone, BotCommands)]
#[command(rename_rule = "snake_case")]
pub enum UserCommands {
    #[default]
    Start,
    ID,
    Invite {
        id: String,
    },
}

pub fn get_process_error(
    bot: Bot,
    chat_id: ChatId,
) -> Box<dyn Fn(String) -> Box<dyn FnOnce(anyhow::Error) -> anyhow::Error + Send> + Send> {
    Box::new(move |msg: String| {
        let bot = bot.clone();
        Box::new(move |e: anyhow::Error| -> anyhow::Error {
            let rt = tokio::runtime::Handle::current();
            let text = format!("{}", e);
            let m = msg.clone();
            rt.spawn(async move {
                let _ = bot
                    .send_message(chat_id, format!("{}: {}", m, text))
                    .send()
                    .await;
            });
            anyhow!("{}. Cause: {}", msg, e)
        })
    })
}

pub async fn on_command(
    bot: Bot,
    msg: Message,
    storage: StoragePtr,
    cfg: CfgPtr,
    cmd: UserCommands,
) -> Result<()> {
    let chat_id = msg.chat.id;
    let user_id = UserId(chat_id.0 as u64);
    let process_error = get_process_error(bot.clone(), chat_id);
    let user_status = storage.get_user_status(user_id).await?;

    match cmd {
        UserCommands::Start => match user_status {
            UserStatus::Granted => {
                let keyboard: Vec<Vec<InlineKeyboardButton>> =
                    vec![vec![InlineKeyboardButton::callback(
                        "Manage profiles".to_string(),
                        serde_json::to_string(&UserCallbackQuery::ManageProfiles {}).unwrap(),
                    )]];
                bot.send_message(chat_id, "Ready to go")
                    .reply_markup(InlineKeyboardMarkup::new(keyboard))
                    .send()
                    .await?;
            }
            UserStatus::None => {
                let keyboard: Vec<Vec<InlineKeyboardButton>> =
                    vec![vec![InlineKeyboardButton::callback(
                        "Request acceess".to_string(),
                        serde_json::to_string(&UserCallbackQuery::RequestAccess {}).unwrap(),
                    )]];
                bot.send_message(chat_id, "Access denied")
                    .reply_markup(InlineKeyboardMarkup::new(keyboard))
                    .send()
                    .await?;
            }
            _ => {
                bot.send_message(chat_id, "Access denied").send().await?;
                return Ok(());
            }
        },
        UserCommands::ID => {
            bot.send_message(msg.chat.id, format!("Your id: {}", msg.chat.id))
                .send()
                .await?;
        }
        UserCommands::Invite { id } => match user_status {
            UserStatus::Granted => {
                bot.send_message(chat_id, "You are already has access")
                    .send()
                    .await?;
                return Ok(());
            }
            UserStatus::Restricted => {
                bot.send_message(chat_id, "Go away").send().await?;
                return Ok(());
            }
            _ => {
                let invite = Invite { id };
                storage
                    .activate_user(user_id, invite)
                    .await
                    .map_err(process_error("Failed to apply invite".into()))?;

                bot.send_message(chat_id, "Access granted").send().await?;
            }
        },
    }
    Ok(())
}

#[derive(Serialize, Deserialize, Debug)]
pub enum ManageProfileAction {
    Delete,
    GetText,
    GetFile,
    GetQR,
}

#[derive(Serialize, Deserialize, Debug)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum UserCallbackQuery {
    ManageProfiles,
    ListProfiles,
    AddProfile,
    GetProfileManager {
        name: String,
    },
    ManageProfile {
        name: String,
        action: ManageProfileAction,
    },
    RequestAccess,
}

pub async fn on_callback_query(
    cq: CallbackQuery,
    bot: Bot,
    storage: StoragePtr,
    cfg: CfgPtr,
    add_profile_dialogue_storage: Arc<InMemStorage<AddProfileDialogueState>>,
) -> Result<()> {
    if let Some(data) = cq.data {
        let callback_query = serde_json::from_str::<UserCallbackQuery>(&data)?;
        let process_error = get_process_error(bot.clone(), cq.from.id.into());
        let user_id = cq.from.id;
        let chat_id = ChatId::from(user_id);
        let sync_server_config = || sync_config(&storage, &cfg);

        match callback_query {
            UserCallbackQuery::GetProfileManager { name } => {
                let _ = storage
                    .get_user_profile(user_id, &name)
                    .await
                    .map_err(process_error("Could not get user profile".into()))?;

                let keyboard: Vec<Vec<InlineKeyboardButton>> = vec![
                    vec![InlineKeyboardButton::callback(
                        "Delete profile",
                        serde_json::to_string(&UserCallbackQuery::ManageProfile {
                            name: name.clone(),
                            action: ManageProfileAction::Delete,
                        })
                        .unwrap(),
                    )],
                    vec![InlineKeyboardButton::callback(
                        "Get profile as text",
                        serde_json::to_string(&UserCallbackQuery::ManageProfile {
                            name: name.clone(),
                            action: ManageProfileAction::GetText,
                        })
                        .unwrap(),
                    )],
                    vec![InlineKeyboardButton::callback(
                        "... as QR",
                        serde_json::to_string(&UserCallbackQuery::ManageProfile {
                            name: name.clone(),
                            action: ManageProfileAction::GetQR,
                        })
                        .unwrap(),
                    )],
                    vec![InlineKeyboardButton::callback(
                        "... as file",
                        serde_json::to_string(&UserCallbackQuery::ManageProfile {
                            name: name.clone(),
                            action: ManageProfileAction::GetFile,
                        })
                        .unwrap(),
                    )],
                ];
                bot.edit_message_text(
                    user_id,
                    cq.message.unwrap().id,
                    format!("Manage profile {name}"),
                )
                .reply_markup(InlineKeyboardMarkup::new(keyboard))
                .send()
                .await?;
            }
            UserCallbackQuery::ListProfiles => {
                let profiles = storage
                    .get_user_profiles(user_id)
                    .await
                    .map_err(process_error("Could not fetch user profiles".into()))?;
                if profiles.is_empty() {
                    bot.send_message(user_id, "There is no available profiles")
                        .send()
                        .await?;
                    return Ok(());
                }

                let mut keyboard: Vec<Vec<InlineKeyboardButton>> = vec![];
                for profiles in profiles.chunks(2) {
                    let row = profiles
                        .iter()
                        .map(|p| {
                            InlineKeyboardButton::callback(
                                p.name.clone(),
                                serde_json::to_string(&UserCallbackQuery::GetProfileManager {
                                    name: p.name.clone(),
                                })
                                .unwrap(),
                            )
                        })
                        .collect();
                    keyboard.push(row);
                }

                bot.edit_message_text(user_id, cq.message.unwrap().id, "Your profiles")
                    .reply_markup(InlineKeyboardMarkup::new(keyboard))
                    .send()
                    .await?;
            }
            UserCallbackQuery::ManageProfiles => {
                let keyboard: Vec<Vec<InlineKeyboardButton>> = vec![
                    vec![InlineKeyboardButton::callback(
                        "Add profile",
                        serde_json::to_string(&UserCallbackQuery::AddProfile {}).unwrap(),
                    )],
                    vec![InlineKeyboardButton::callback(
                        "Get profile",
                        serde_json::to_string(&UserCallbackQuery::ListProfiles {}).unwrap(),
                    )],
                ];

                bot.edit_message_text(user_id, cq.message.unwrap().id, "Manage profiles")
                    .reply_markup(InlineKeyboardMarkup::new(keyboard))
                    .send()
                    .await?;
            }
            UserCallbackQuery::AddProfile => {
                add_profile_dialogue_storage
                    .update_dialogue(user_id.into(), AddProfileDialogueState::WaitForName)
                    .await?;
                bot.edit_message_text(user_id, cq.message.unwrap().id, "Send profile name")
                    .send()
                    .await?;
            }
            UserCallbackQuery::ManageProfile { name, action } => match action {
                ManageProfileAction::Delete => {
                    storage
                        .delete_user_profile(user_id, &name)
                        .await
                        .map_err(process_error("Could not delete profile".into()))?;
                    sync_server_config()
                        .await
                        .map_err(process_error("Could not sync server config".into()))?;
                    bot.send_message(
                        user_id,
                        format!("Profile with name {name} deleted successfully"),
                    )
                    .send()
                    .await?;
                }
                ManageProfileAction::GetText => {
                    let profile = storage
                        .get_user_profile(user_id.into(), &name)
                        .await
                        .map_err(process_error("Could not get user profile".into()))?;

                    let peer_cfg = PeerConfig::new(&profile, &cfg)
                        .map_err(process_error("Could not build peer config".into()))?;
                    let profile_text = build_peer_config(&peer_cfg)
                        .map_err(|e| anyhow!(e))
                        .map_err(process_error("Could not build client config".into()))?;

                    bot.send_message(user_id, format!("Config:\n\n```\n{}\n```", profile_text))
                        .parse_mode(ParseMode::MarkdownV2)
                        .send()
                        .await?;
                }
                ManageProfileAction::GetFile => {
                    let profile = storage
                        .get_user_profile(user_id.into(), &name)
                        .await
                        .map_err(process_error("Could not get user profile".into()))?;

                    let peer_cfg = PeerConfig::new(&profile, &cfg)?;
                    let profile_text = build_peer_config(&peer_cfg)
                        .map_err(|e| anyhow!(e))
                        .map_err(process_error("Could not build client config".into()))?;

                    let data = bytes::Bytes::from(profile_text);

                    bot.send_document(user_id, InputFile::memory(data))
                        .send()
                        .await?;
                }
                ManageProfileAction::GetQR => {
                    let profile = storage
                        .get_user_profile(user_id.into(), &name)
                        .await
                        .map_err(process_error("Could not get user profile".into()))?;

                    let peer_cfg = PeerConfig::new(&profile, &cfg)
                        .map_err(process_error("Could not build peer config".into()))?;
                    let profile_text = build_peer_config(&peer_cfg)
                        .map_err(|e| anyhow!(e))
                        .map_err(process_error("Could not build client config".into()))?;

                    let path = get_qr_path();

                    let cmd = Command::new("qrencode")
                        .arg("-o")
                        .arg(&path)
                        .stdin(Stdio::piped())
                        .stdout(Stdio::piped())
                        .spawn()
                        .map_err(|e| {
                            process_error("Could not generate QR code".into())(anyhow!(e))
                        })?;

                    cmd.stdin
                        .as_ref()
                        .unwrap()
                        .write_all(profile_text.as_bytes())
                        .map_err(|e| {
                            process_error("Could not generate QR code".into())(anyhow!(e))
                        })?;

                    let _ = cmd.wait_with_output()?;

                    bot.send_photo(user_id, InputFile::file(&path))
                        .send()
                        .await?;
                }
            },
            UserCallbackQuery::RequestAccess => {
                let user_status = storage.get_user_status(user_id).await?;
                match user_status {
                    UserStatus::Restricted => {
                        bot.send_message(chat_id, "Go away").send().await?;
                        return Ok(());
                    }
                    UserStatus::Requested => {
                        bot.send_message(chat_id, "You are already requested for access")
                            .send()
                            .await?;
                        return Ok(());
                    }
                    UserStatus::Granted => {
                        bot.send_message(chat_id, "You are already has access")
                            .send()
                            .await?;
                        return Ok(());
                    }
                    UserStatus::None => {
                        storage
                            .update_user_status(user_id, UserStatus::Requested)
                            .await?;
                        bot.send_message(chat_id, "Request sent").send().await?;
                        bot.send_message(ChatId(cfg.admin_id), "New access request were recieved")
                            .send()
                            .await?;
                        return Ok(());
                    }
                }
            }
        }
    }
    Ok(())
}

fn get_qr_path() -> String {
    let mut rng = rand::thread_rng();
    let num: u64 = rng.gen();
    let path = format!("/tmp/qr-{}.png", num);
    path
}
