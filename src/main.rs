mod cfg;
mod control_client;
mod handlers;
mod rpc;
mod statistics;
mod storage;
mod wireguard;

use anyhow::Result;
use std::sync::Arc;
use teloxide::{dispatching::dialogue::InMemStorage, prelude::*};
use tracing::level_filters::LevelFilter;
use tracing_subscriber::{prelude::*, registry::Registry};

fn main() -> Result<()> {
    let fmt_layer = tracing_subscriber::fmt::layer()
        .with_target(false)
        .with_filter(LevelFilter::DEBUG);

    Registry::default().with(fmt_layer).try_init().unwrap();

    let rt = tokio::runtime::Runtime::new()?;
    let result: Result<()> = rt.block_on(async move {
        let service_config = Arc::new(cfg::get_config()?);
        let storage = Arc::new(storage::Storage::new().await?);
        let bot = Bot::new(service_config.bot_token.clone());

        control_client::start_wireguard_server(&service_config).await?;
        control_client::sync_config(&storage, &service_config).await?;

        Dispatcher::builder(bot, handlers::get_handler(service_config.clone()))
            .dependencies(dptree::deps![
                service_config.clone(),
                storage.clone(),
                InMemStorage::<handlers::AddProfileDialogueState>::new()
            ])
            .enable_ctrlc_handler()
            .build()
            .dispatch()
            .await;
        Ok(())
    });

    if let Err(e) = result {
        eprintln!("Program finished with error: {}", e);
        e.chain()
            .skip(1)
            .for_each(|cause| eprintln!("because: {}", cause));
    }

    Ok(())
}
