mod importer_online;

use stratus::config::ExternalRelayerConfig;
#[cfg(feature = "metrics")]
use stratus::infra::metrics;
use stratus::GlobalServices;
use stratus::GlobalState;

const TASK_NAME: &str = "relayer";

fn main() -> anyhow::Result<()> {
    let global_services = GlobalServices::<ExternalRelayerConfig>::init()?;
    global_services.runtime.block_on(run(global_services.config))
}

async fn run(config: ExternalRelayerConfig) -> anyhow::Result<()> {
    tracing::info!(?TASK_NAME, "starting");

    // init services
    let backoff = config.relayer.backoff;
    let relayer = config.relayer.init().await?;

    loop {
        if GlobalState::warn_if_shutdown(TASK_NAME) {
            return Ok(());
        };

        #[cfg(feature = "metrics")]
        let start = metrics::now();
        let block_number = match relayer.relay_next_block().await {
            Ok(block_number) => {
                #[cfg(feature = "metrics")]
                metrics::inc_relay_next_block(start.elapsed());
                block_number
            }
            Err(err) => {
                tracing::error!(?err, "error relaying next block");
                continue;
            }
        };

        match block_number {
            Some(block_number) => tracing::info!(number = %block_number, "relayed"),
            None => {
                tracing::info!("no pending block found");
                tokio::time::sleep(backoff).await;
            }
        };
    }
}