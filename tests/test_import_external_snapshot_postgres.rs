mod test_import_external_snapshot_common;

use std::time::Duration;

use stratus::eth::storage::PermanentStorage;
use stratus::eth::storage::PostgresPermanentStorage;
use stratus::eth::storage::PostgresPermanentStorageConfig;
use stratus::infra::docker::Docker;
use test_import_external_snapshot_common as common;

#[tokio::test]
async fn test_import_external_snapshot_with_postgres() {
    let docker = Docker::default();
    let _prom_guard = docker.start_prometheus();
    let _pg_guard = docker.start_postgres();

    let (config, block, receipts, snapshot) = common::init_config_and_data();
    let (accounts, slots) = common::filter_accounts_and_slots(snapshot);

    let pg = PostgresPermanentStorage::new(PostgresPermanentStorageConfig {
        url: docker.postgres_connection_url().to_string(),
        connections: 5,
        acquire_timeout: Duration::from_secs(10),
    })
    .await
    .unwrap();
    pg.save_accounts(accounts.clone()).await.unwrap();

    let mut tx = pg.pool.begin().await.unwrap();
    for (address, slot) in slots {
        sqlx::query("insert into account_slots(idx, value, account_address, creation_block) values($1, $2, $3, $4)")
            .bind(slot.index.clone())
            .bind(slot.value.clone())
            .bind(&address)
            .bind(0)
            .execute(&mut *tx)
            .await
            .unwrap();
    }
    tx.commit().await.unwrap();

    common::execute_test("PostgreSQL", &config, &docker, pg, block, receipts).await;
}