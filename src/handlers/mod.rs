mod add_profile_dialogue;
mod admin;
mod user;

use std::sync::Arc;

use anyhow::Result;
use teloxide::{
    dispatching::{dialogue::InMemStorage, DpHandlerDescription, HandlerExt, UpdateFilterExt},
    prelude::*,
};

use crate::{
    cfg::CfgPtr,
    handlers::add_profile_dialogue::{handle_wait_for_name, AddProfileDialogue},
    storage::StoragePtr,
};

pub use add_profile_dialogue::AddProfileDialogueState;

pub fn get_handler(
    cfg: CfgPtr,
) -> Handler<'static, DependencyMap, Result<()>, DpHandlerDescription> {
    let admin_id = ChatId(cfg.admin_id);

    async fn user_branch(bot: Bot, msg: Message, storage: StoragePtr, cfg: CfgPtr) -> Result<()> {
        if !msg.chat.is_private() {
            bot.send_message(msg.chat.id, "Current bot works only in private chat")
                .send()
                .await?;
            return Ok(());
        }

        Ok(())
    }

    async fn filter_non_empty_add_profile_dialogue(
        storage: Arc<InMemStorage<AddProfileDialogueState>>,
        msg: Message,
    ) -> bool {
        if !msg.chat.is_private() {
            return false;
        }

        let dialogue = AddProfileDialogue::new(storage, msg.chat.id);

        dialogue.get().await.unwrap_or_default() != Some(AddProfileDialogueState::NotStarted)
    }

    let msg_handler = Update::filter_message()
        .enter_dialogue::<Message, InMemStorage<AddProfileDialogueState>, AddProfileDialogueState>()
        .branch(
            dptree::filter_async(filter_non_empty_add_profile_dialogue).branch(
                dptree::case![AddProfileDialogueState::WaitForName].endpoint(handle_wait_for_name),
            ),
        )
        .branch(
            dptree::entry()
                .filter_command::<user::UserCommands>()
                .endpoint(user::on_command),
        )
        .branch(
            dptree::filter(move |msg: Message| msg.chat.is_private() && msg.chat.id == admin_id)
                .filter_command::<admin::AdminCommands>()
                .endpoint(admin::on_command),
        );

    let callback_query_handler = Update::filter_callback_query()
        .branch(
            dptree::filter(move |cq: CallbackQuery| {
                ChatId(cq.from.id.0 as i64) == admin_id
                    && serde_json::from_str::<admin::AdminCallbackQuery>(
                        &cq.data.unwrap_or(String::new()),
                    )
                    .is_ok()
            })
            .endpoint(admin::on_callback_query),
        )
        .branch(
            dptree::filter(move |cq: CallbackQuery| {
                serde_json::from_str::<user::UserCallbackQuery>(&cq.data.unwrap_or(String::new()))
                    .is_ok()
            })
            .endpoint(user::on_callback_query),
        );

    dptree::entry()
        .branch(msg_handler)
        .branch(callback_query_handler)
}
