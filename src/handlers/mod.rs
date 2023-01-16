mod user;
mod admin;
mod add_profile_dialogue;

use std::sync::Arc;

use teloxide::{prelude::*, dispatching::{UpdateFilterExt, HandlerExt, dialogue::InMemStorage, DpHandlerDescription}};
use anyhow::Result;

use crate::{cfg::CfgPtr, storage::StoragePtr, handlers::add_profile_dialogue::{AddProfileDialogue, handle_wait_for_name}};

pub use add_profile_dialogue::AddProfileDialogueState;

pub fn get_handler(cfg: CfgPtr) -> Handler<'static, DependencyMap, Result<()>, DpHandlerDescription> {
    let admin_id = ChatId(cfg.admin_id as i64);
    
    async fn user_branch(bot: Bot, msg: Message, storage: StoragePtr, cfg: CfgPtr) -> Result<()> {
        if !msg.chat.is_private() {
            bot.send_message(msg.chat.id, "Current bot works only in private chat").send().await?;
            return Ok(());
        }

        Ok(())
    }
    
    async fn filter_non_empty_add_profile_dialogue(storage: Arc<InMemStorage<AddProfileDialogueState>>, msg: Message) -> bool {
        if !msg.chat.is_private() {
            return false;
        }
        
        let dialogue = AddProfileDialogue::new(storage, msg.chat.id);

        dialogue.get().await.unwrap_or_default() != Some(AddProfileDialogueState::NotStarted)
    }

    let msg_handler = Update::filter_message()
        .enter_dialogue::<Message, InMemStorage<AddProfileDialogueState>, AddProfileDialogueState>()
        .branch(
            dptree::filter_async(filter_non_empty_add_profile_dialogue)
                .branch(dptree::case![AddProfileDialogueState::WaitForName].endpoint(handle_wait_for_name))
        )
        
        .branch(
            dptree::entry()
                .filter_command::<user::UserCommands>().endpoint(user::on_command)
        )
        .branch(
            dptree::filter(move |msg: Message| {
                msg.chat.is_private() && msg.chat.id == admin_id
            })
            .filter_command::<admin::AdminCommands>().endpoint(admin::on_command)
        );

    let callback_query_handler = Update::filter_callback_query()
        .endpoint(user::on_callback_query);

    dptree::entry().branch(msg_handler).branch(callback_query_handler)
}