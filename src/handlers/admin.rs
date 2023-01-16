use teloxide::{
    prelude::*, 
    macros::BotCommands, types::ParseMode
};
use anyhow::Result;

use crate::{
    storage::StoragePtr,
    handlers::user::get_process_error
};

#[derive(Default, Clone, BotCommands)]
#[command(rename_rule = "snake_case")]
pub enum AdminCommands {
    #[default]
    Admin,
    NewInvite,
    RevokeInvites,
}

pub async fn on_command(bot: Bot, msg: Message, storage: StoragePtr, cmd: AdminCommands) -> Result<()> {
    let chat_id = msg.chat.id;
    let process_error = get_process_error(bot.clone(), chat_id);
    match cmd {
        AdminCommands::Admin => {
            bot.send_message(chat_id, "Hi, admin!").send().await?;
        },
        AdminCommands::NewInvite => {
            let invite = storage.create_invite_code().await
                .map_err(process_error("Failed to create invite code".into()))?;
            let text = format!("New invite code was created: `{}`", invite.id);
            bot.send_message(chat_id, text)
                .parse_mode(ParseMode::MarkdownV2)
                .send().await?;
        },
        AdminCommands::RevokeInvites => {
            storage.revoke_all_invite_codes().await
                .map_err(process_error("Failed to revoke all invite codes".into()))?;
            bot.send_message(chat_id, "All invited codes were revoked").send().await?;
        },
    }
    Ok(())
}
