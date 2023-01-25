use std::sync::Arc;

use anyhow::Result;
use teloxide::{
    dispatching::dialogue::{Dialogue, InMemStorage, Storage},
    prelude::*,
};

use crate::{cfg::CfgPtr, control_client, storage::StoragePtr};

#[derive(Default, Debug, Clone, PartialEq, Eq)]
pub enum AddProfileDialogueState {
    #[default]
    NotStarted,
    WaitForName,
}

pub type AddProfileDialogue =
    Dialogue<AddProfileDialogueState, InMemStorage<AddProfileDialogueState>>;

pub async fn handle_wait_for_name(
    bot: Bot,
    msg: Message,
    storage: StoragePtr,
    cfg: CfgPtr,
    add_profile_dialogue_storage: Arc<InMemStorage<AddProfileDialogueState>>,
) -> Result<()> {
    let name = msg.text().unwrap_or_default().to_owned();
    if name.is_empty() {
        bot.send_message(msg.chat.id, "You should send profile name to create")
            .send()
            .await?;
        return Ok(());
    }

    if let Ok(_) = storage
        .add_profile(&name, UserId(msg.chat.id.0 as u64))
        .await
    {
        add_profile_dialogue_storage
            .remove_dialogue(msg.chat.id)
            .await?;
        control_client::sync_config(&storage, &cfg).await?;
        bot.send_message(
            msg.chat.id,
            format!("Profile with name {} was created", name),
        )
        .send()
        .await?;
    } else {
        bot.send_message(
            msg.chat.id,
            format!("Profile with name {} is already exists", name),
        )
        .send()
        .await?;
    }
    Ok(())
}
