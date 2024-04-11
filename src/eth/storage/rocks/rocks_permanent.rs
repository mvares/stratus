use std::sync::atomic::AtomicU64;
use std::sync::atomic::AtomicUsize;
use std::sync::atomic::Ordering;
use std::sync::Arc;
use std::sync::Mutex;
use std::time::Instant;

use anyhow::Context;
use async_trait::async_trait;
use futures::future::join_all;
use once_cell::sync::Lazy;

use super::rocks_state::RocksStorageState;
use crate::eth::primitives::Account;
use crate::eth::primitives::Address;
use crate::eth::primitives::Block;
use crate::eth::primitives::BlockNumber;
use crate::eth::primitives::BlockSelection;
use crate::eth::primitives::ExecutionAccountChanges;
use crate::eth::primitives::ExecutionConflicts;
use crate::eth::primitives::ExecutionConflictsBuilder;
use crate::eth::primitives::Hash;
use crate::eth::primitives::LogFilter;
use crate::eth::primitives::LogMined;
use crate::eth::primitives::Slot;
use crate::eth::primitives::SlotIndex;
use crate::eth::primitives::SlotSample;
use crate::eth::primitives::StoragePointInTime;
use crate::eth::primitives::TransactionMined;
use crate::eth::storage::rocks::rocks_state::AccountInfo;
use crate::eth::storage::PermanentStorage;
use crate::eth::storage::StorageError;

/// used for multiple purposes, such as TPS counting and backup management
const TRANSACTION_LOOP_THRESHOLD: usize = 210_000;

static TRANSACTIONS_COUNT: AtomicUsize = AtomicUsize::new(0);
static START_TIME: Lazy<Mutex<Instant>> = Lazy::new(|| Mutex::new(Instant::now()));

#[derive(Debug)]
pub struct RocksPermanentStorage {
    state: RocksStorageState,
    block_number: AtomicU64,
}

impl RocksPermanentStorage {
    pub fn new() -> anyhow::Result<Self> {
        tracing::info!("starting rocksdb storage");

        let state = RocksStorageState::new();
        state.sync_data()?;
        let block_number = state.preload_block_number()?;
        Ok(Self { state, block_number })
    }

    // -------------------------------------------------------------------------
    // State methods
    // -------------------------------------------------------------------------

    pub fn clear(&self) {
        self.state.clear().unwrap();
        self.block_number.store(0, Ordering::SeqCst);
    }

    fn check_conflicts(state: &RocksStorageState, account_changes: &[ExecutionAccountChanges]) -> Option<ExecutionConflicts> {
        let mut conflicts = ExecutionConflictsBuilder::default();

        for change in account_changes {
            let address = &change.address;

            if let Some(account) = state.accounts.get(address) {
                // check account info conflicts
                if let Some(original_nonce) = change.nonce.take_original_ref() {
                    let account_nonce = &account.nonce;
                    if original_nonce != account_nonce {
                        conflicts.add_nonce(address.clone(), account_nonce.clone(), original_nonce.clone());
                    }
                }
                if let Some(original_balance) = change.balance.take_original_ref() {
                    let account_balance = &account.balance;
                    if original_balance != account_balance {
                        conflicts.add_balance(address.clone(), account_balance.clone(), original_balance.clone());
                    }
                }
                // check slots conflicts
                for (slot_index, slot_change) in &change.slots {
                    if let Some(value) = state.account_slots.get(&(address.clone(), slot_index.clone())) {
                        if let Some(original_slot) = slot_change.take_original_ref() {
                            let account_slot_value = value.clone();
                            if original_slot.value != account_slot_value {
                                conflicts.add_slot(address.clone(), slot_index.clone(), account_slot_value, original_slot.value.clone());
                            }
                        }
                    }
                }
            }
        }
        conflicts.build()
    }
}

#[async_trait]
impl PermanentStorage for RocksPermanentStorage {
    async fn allocate_evm_thread_resources(&self) -> anyhow::Result<()> {
        Ok(())
    }

    // -------------------------------------------------------------------------
    // Block number operations
    // -------------------------------------------------------------------------

    async fn read_mined_block_number(&self) -> anyhow::Result<BlockNumber> {
        Ok(self.block_number.load(Ordering::SeqCst).into())
    }

    async fn increment_block_number(&self) -> anyhow::Result<BlockNumber> {
        let next = self.block_number.fetch_add(1, Ordering::SeqCst) + 1;
        Ok(next.into())
    }

    async fn set_mined_block_number(&self, number: BlockNumber) -> anyhow::Result<()> {
        self.block_number.store(number.as_u64(), Ordering::SeqCst);
        Ok(())
    }

    // -------------------------------------------------------------------------
    // State operations
    // ------------------------------------------------------------------------

    async fn maybe_read_account(&self, address: &Address, point_in_time: &StoragePointInTime) -> anyhow::Result<Option<Account>> {
        Ok(self.state.read_account(address, point_in_time))
    }

    async fn maybe_read_slot(&self, address: &Address, slot_index: &SlotIndex, point_in_time: &StoragePointInTime) -> anyhow::Result<Option<Slot>> {
        tracing::debug!(%address, %slot_index, ?point_in_time, "reading slot");
        Ok(self.state.read_slot(address, slot_index, point_in_time))
    }

    async fn read_block(&self, selection: &BlockSelection) -> anyhow::Result<Option<Block>> {
        Ok(self.state.read_block(selection))
    }

    async fn read_mined_transaction(&self, hash: &Hash) -> anyhow::Result<Option<TransactionMined>> {
        tracing::debug!(%hash, "reading transaction");
        self.state.read_transaction(hash)
    }

    async fn read_logs(&self, filter: &LogFilter) -> anyhow::Result<Vec<LogMined>> {
        tracing::debug!(?filter, "reading logs");
        self.state.read_logs(filter)
    }

    async fn save_block(&self, block: Block) -> anyhow::Result<(), StorageError> {
        // check conflicts before persisting any state changes
        let account_changes = block.compact_account_changes();
        if let Some(conflicts) = Self::check_conflicts(&self.state, &account_changes) {
            return Err(StorageError::Conflict(conflicts));
        }

        let mut futures = Vec::with_capacity(9);

        let mut txs_batch = vec![];
        let mut logs_batch = vec![];
        for transaction in block.transactions.clone() {
            txs_batch.push((transaction.input.hash.clone(), transaction.block_number));
            for log in transaction.logs {
                logs_batch.push(((transaction.input.hash.clone(), log.log_index), transaction.block_number));
            }
        }

        let txs_rocks = Arc::clone(&self.state.transactions);
        let logs_rocks = Arc::clone(&self.state.logs);
        futures.push(tokio::task::spawn_blocking(move || txs_rocks.insert_batch(txs_batch, None)));
        futures.push(tokio::task::spawn_blocking(move || logs_rocks.insert_batch(logs_batch, None)));

        // save block
        let number = *block.number();
        let hash = block.hash().clone();

        let blocks_by_number = Arc::clone(&self.state.blocks_by_number);
        let blocks_by_hash = Arc::clone(&self.state.blocks_by_hash);
        let mut block_without_changes = block.clone();
        for transaction in &mut block_without_changes.transactions {
            transaction.execution.changes = vec![];
        }
        let hash_clone = hash.clone();
        futures.push(tokio::task::spawn_blocking(move || blocks_by_number.insert(number, block_without_changes)));
        futures.push(tokio::task::spawn_blocking(move || blocks_by_hash.insert(hash_clone, number)));

        futures.append(
            &mut self
                .state
                .update_state_with_execution_changes(&account_changes, number)
                .context("failed to update state with execution changes")?,
        );

        // TPS Calculation and Printing
        futures.push(tokio::task::spawn_blocking(move || {
            let previous_count = TRANSACTIONS_COUNT.load(Ordering::Relaxed);
            let current_count = TRANSACTIONS_COUNT.fetch_add(block.transactions.len(), Ordering::Relaxed);
            let elapsed_time = START_TIME.lock().unwrap().elapsed().as_secs_f64();
            let multiple_to_print = TRANSACTION_LOOP_THRESHOLD / 8;

            // for every multiple of transactions, print the TPS
            if previous_count % multiple_to_print > current_count % multiple_to_print {
                let total_transactions = TRANSACTIONS_COUNT.load(Ordering::Relaxed);
                let tps = total_transactions as f64 / elapsed_time;
                //TODO replace this with metrics or do a cfg feature to enable/disable
                println!("Transactions per second: {:.2} @ block {}", tps, block.number());
            }

            // for every multiple of TRANSACTION_LOOP_THRESHOLD transactions, reset the counter
            if previous_count % TRANSACTION_LOOP_THRESHOLD > current_count % TRANSACTION_LOOP_THRESHOLD {
                TRANSACTIONS_COUNT.store(0, Ordering::Relaxed);
                let mut start_time = START_TIME.lock().unwrap();
                *start_time = Instant::now();
            }
        }));

        join_all(futures).await;
        Ok(())
    }

    async fn after_commit_hook(&self) -> anyhow::Result<()> {
        Ok(())
    }

    async fn save_accounts(&self, accounts: Vec<Account>) -> anyhow::Result<()> {
        tracing::debug!(?accounts, "saving initial accounts");

        for account in accounts {
            self.state.accounts.insert(
                account.address.clone(),
                AccountInfo {
                    balance: account.balance.clone(),
                    nonce: account.nonce.clone(),
                    bytecode: account.bytecode.clone(),
                    code_hash: account.code_hash.clone(),
                },
            );

            self.state.accounts_history.insert(
                (account.address.clone(), 0.into()),
                AccountInfo {
                    balance: account.balance.clone(),
                    nonce: account.nonce.clone(),
                    bytecode: account.bytecode.clone(),
                    code_hash: account.code_hash.clone(),
                },
            );
        }

        Ok(())
    }

    async fn reset_at(&self, block_number: BlockNumber) -> anyhow::Result<()> {
        // reset block number
        let block_number_u64: u64 = block_number.into();
        let _ = self.block_number.fetch_update(Ordering::SeqCst, Ordering::SeqCst, |current| {
            if block_number_u64 <= current {
                Some(block_number_u64)
            } else {
                None
            }
        });

        self.state.reset_at(block_number)
    }

    async fn read_slots_sample(&self, _start: BlockNumber, _end: BlockNumber, _max_samples: u64, _seed: u64) -> anyhow::Result<Vec<SlotSample>> {
        todo!()
    }
}