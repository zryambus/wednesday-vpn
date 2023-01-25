use anyhow::Result;
use serde::{Deserialize, Serialize};
use serde_json;
use teloxide::{
    macros::BotCommands,
    prelude::*,
    types::{InlineKeyboardButton, InlineKeyboardMarkup, ParseMode},
};

use crate::{
    handlers::user::get_process_error,
    storage::{StoragePtr, User, UserStatus},
    control_client::get_statistics,
};

#[derive(Default, Clone, BotCommands)]
#[command(rename_rule = "snake_case")]
pub enum AdminCommands {
    #[default]
    Admin,
    NewInvite,
    RevokeInvites,
    ReviewRequests,
    Statistics,
}

pub async fn on_command(
    bot: Bot,
    msg: Message,
    storage: StoragePtr,
    cmd: AdminCommands,
) -> Result<()> {
    let chat_id = msg.chat.id;
    // let user_id = UserId(chat_id.0 as u64);
    let process_error = get_process_error(bot.clone(), chat_id);
    match cmd {
        AdminCommands::Admin => {
            bot.send_message(chat_id, "Hi, admin!").send().await?;
        }
        AdminCommands::NewInvite => {
            let invite = storage
                .create_invite_code()
                .await
                .map_err(process_error("Failed to create invite code".into()))?;
            let text = format!("New invite code was created: `{}`", invite.id);
            bot.send_message(chat_id, text)
                .parse_mode(ParseMode::MarkdownV2)
                .send()
                .await?;
        }
        AdminCommands::RevokeInvites => {
            storage
                .revoke_all_invite_codes()
                .await
                .map_err(process_error("Failed to revoke all invite codes".into()))?;
            bot.send_message(chat_id, "All invited codes were revoked")
                .send()
                .await?;
        }
        AdminCommands::ReviewRequests => {
            let users = storage.get_users_with_requested_access().await?;
            if users.is_empty() {
                bot.send_message(chat_id, "No requests").send().await?;
                return Ok(());
            }

            let mut infos: Vec<(String, UserId)> = vec![];
            for user in users {
                let chat = bot.get_chat(ChatId(user.user_id.parse()?)).await?;
                let user_id = UserId(chat.id.0 as u64);
                let name = format!(
                    "{} @{} {}",
                    chat.first_name().unwrap_or("(No first name)"),
                    chat.username().unwrap_or("(No username)"),
                    chat.last_name().unwrap_or("(No last name)")
                );
                infos.push((name, user_id));
            }

            let mut text = String::new();
            let mut keyboard: Vec<Vec<InlineKeyboardButton>> = vec![];
            for (idx, (name, user_id)) in infos.into_iter().enumerate() {
                text.push_str(&format!("{}. {}\n", idx + 1, name));
                keyboard.push(vec![
                    InlineKeyboardButton::callback(
                        format!("{}. Accept {}", idx + 1, name),
                        serde_json::to_string(&AdminCallbackQuery::AcceptRequest { user_id })
                            .unwrap(),
                    ),
                    InlineKeyboardButton::callback(
                        "Reject".to_string(),
                        serde_json::to_string(&AdminCallbackQuery::RejectRequesst { user_id })
                            .unwrap(),
                    ),
                ])
            }

            bot.send_message(chat_id, text)
                .reply_markup(InlineKeyboardMarkup::new(keyboard))
                .send()
                .await?;
        },
        AdminCommands::Statistics => {
            let entries = get_statistics().await?;
            let mut text = String::from("Statistics:\n");
            for entry in entries {
                let profile = storage.get_profile(&entry.pubkey).await?;
                let chat = bot.get_chat(ChatId(profile.user_id.parse()?)).await?;
                let name = format!(
                    "{} @{} {}",
                    chat.first_name().unwrap_or("(No first name)"),
                    chat.username().unwrap_or("(No username)"),
                    chat.last_name().unwrap_or("(No last name)")
                );
                text.push_str(&format!("{} {}\n", name, entry));
            }
            bot.send_message(chat_id, text).send().await?;
        }
    }
    Ok(())
}

#[derive(Serialize, Deserialize, Debug)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum AdminCallbackQuery {
    AcceptRequest { user_id: UserId },
    RejectRequesst { user_id: UserId },
}

pub async fn on_callback_query(cq: CallbackQuery, bot: Bot, storage: StoragePtr) -> Result<()> {
    let query = serde_json::from_str(&cq.data.unwrap())?;
    match query {
        AdminCallbackQuery::AcceptRequest { user_id } => {
            storage
                .update_user_status(user_id, UserStatus::Granted)
                .await?;
            bot.send_message(ChatId::from(user_id), "Access granted")
                .send()
                .await?;
        }
        AdminCallbackQuery::RejectRequesst { user_id } => {
            storage
                .update_user_status(user_id, UserStatus::Restricted)
                .await?;
            bot.send_message(ChatId::from(user_id), "Go away")
                .send()
                .await?;
        }
    }

    Ok(())
}
