use std::collections::HashMap;
use std::collections::HashSet;

use daggy::stable_dag::StableDag;
use daggy::Walker;
use itertools::Itertools;
use petgraph::graph::NodeIndex;
use petgraph::visit::IntoNodeIdentifiers;

use crate::eth::primitives::Address;
use crate::eth::primitives::Index;
use crate::eth::primitives::SlotIndex;
use crate::eth::primitives::TransactionMined;
#[cfg(feature = "metrics")]
use crate::infra::metrics;

pub struct TransactionDag {
    dag: StableDag<TransactionMined, i32>,
}

impl TransactionDag {
    /// Uses the transactions and produces a Dependency DAG (Directed Acyclical Graph).
    /// Each vertex of the graph is a transaction, and two vertices are connected iff they conflict
    /// on either a slot or balance.
    /// The direction of an edge connecting the transactions A and B is always from
    /// `min(A.transaction_index, B.transaction_index)` to `max(A.transaction_index, B.transaction_index)`.
    /// Possible issues: this accounts for writes but not for reads, a transaction that reads a certain
    ///     slot but does not modify it would possibly be impacted by a transaction that does, meaning they
    ///     have a dependency that is not addressed here. Also there is a dependency between contract deployments
    ///     and contract calls that is not taken into consideration yet.
    /// If this algorithm is correct we could do away with StableDag and use StableGraph instead, for better performance
    #[tracing::instrument(skip_all)]
    pub fn new(block_transactions: Vec<TransactionMined>) -> Self {
        #[cfg(feature = "metrics")]
        let start = metrics::now();

        let mut slot_conflicts: HashMap<Index, HashSet<(Address, SlotIndex)>> = HashMap::new();
        let mut balance_conflicts: HashMap<Index, HashSet<Address>> = HashMap::new();
        let mut node_indexes: HashMap<Index, NodeIndex> = HashMap::new();
        let mut dag = StableDag::new();

        for tx in block_transactions.into_iter().sorted_by_key(|tx| tx.transaction_index) {
            let tx_idx = tx.transaction_index;
            for (address, change) in &tx.execution.changes {
                for (idx, slot_change) in &change.slots {
                    if slot_change.is_modified() {
                        slot_conflicts.entry(tx_idx).or_default().insert((*address, *idx));
                    }
                }

                if change.balance.is_modified() {
                    balance_conflicts.entry(tx_idx).or_default().insert(*address);
                }
            }
            let node_idx = dag.add_node(tx);
            node_indexes.insert(tx_idx, node_idx);
        }

        for (i, (tx1, set1)) in slot_conflicts.iter().sorted_by_key(|(idx, _)| **idx).enumerate() {
            for (tx2, set2) in slot_conflicts.iter().sorted_by_key(|(idx, _)| **idx).skip(i + 1) {
                if !set1.is_disjoint(set2) {
                    dag.add_edge(*node_indexes.get(tx1).unwrap(), *node_indexes.get(tx2).unwrap(), 1)
                        .expect("adding an edge between two known vertices should not fail");
                }
            }
        }

        for (i, (tx1, set1)) in balance_conflicts.iter().sorted_by_key(|(idx, _)| **idx).enumerate() {
            for (tx2, set2) in balance_conflicts.iter().sorted_by_key(|(idx, _)| **idx).skip(i + 1) {
                if !set1.is_disjoint(set2) {
                    dag.add_edge(*node_indexes.get(tx1).unwrap(), *node_indexes.get(tx2).unwrap(), 1)
                        .expect("adding an edge between two known vertices should not fail");
                }
            }
        }

        #[cfg(feature = "metrics")]
        metrics::inc_compute_tx_dag(start.elapsed());

        Self { dag }
    }

    /// Takes the roots (vertices with no parents) from the DAG, removing them from the graph,
    /// and by extension creating new roots for a future call. Returns `None` if the graph
    /// is empty.
    #[tracing::instrument(skip_all)]
    pub fn take_roots(&mut self) -> Option<Vec<TransactionMined>> {
        #[cfg(feature = "metrics")]
        let start = metrics::now();
        let dag = &mut self.dag;

        let mut root_indexes = vec![];
        for index in dag.node_identifiers() {
            if dag.parents(index).walk_next(dag).is_none() {
                root_indexes.push(index);
            }
        }

        let mut roots = vec![];
        while let Some(root) = root_indexes.pop() {
            roots.push(dag.remove_node(root).expect("removing a known vertex should not fail"));
        }

        #[cfg(feature = "metrics")]
        metrics::inc_take_roots(start.elapsed());

        if roots.is_empty() {
            None
        } else {
            Some(roots)
        }
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashSet;

    use super::TransactionDag;
    use crate::eth::primitives::Address;
    use crate::eth::primitives::Bytes;
    use crate::eth::primitives::CodeHash;
    use crate::eth::primitives::EvmExecution;
    use crate::eth::primitives::ExecutionAccountChanges;
    use crate::eth::primitives::ExecutionResult;
    use crate::eth::primitives::ExecutionValueChange;
    use crate::eth::primitives::Gas;
    use crate::eth::primitives::Hash;
    use crate::eth::primitives::Slot;
    use crate::eth::primitives::SlotIndex;
    use crate::eth::primitives::TransactionInput;
    use crate::eth::primitives::TransactionMined;
    use crate::eth::primitives::UnixTime;
    const ADDRESS: Address = Address::ZERO;

    fn create_tx(changed_slots_inidices: HashSet<SlotIndex>, tx_idx: u64) -> TransactionMined {
        let execution_changes = ExecutionAccountChanges {
            new_account: false,
            address: ADDRESS,
            nonce: ExecutionValueChange::default(),
            balance: ExecutionValueChange::default(),
            bytecode: ExecutionValueChange::default(),
            code_hash: CodeHash::default(),
            static_slot_indexes: ExecutionValueChange::default(),
            mapping_slot_indexes: ExecutionValueChange::default(),
            slots: changed_slots_inidices
                .into_iter()
                .map(|index| (index, ExecutionValueChange::from_modified(Slot { index, value: 0.into() })))
                .collect(),
        };
        let execution = EvmExecution {
            block_timestamp: UnixTime::default(),
            receipt_applied: false,
            result: ExecutionResult::Success,
            output: Bytes::default(),
            logs: vec![],
            gas: Gas::default(),
            changes: [(ADDRESS, execution_changes)].into_iter().collect(),
            deployed_contract_address: None,
        };

        TransactionMined {
            input: TransactionInput::default(),
            execution,
            logs: vec![],
            transaction_index: tx_idx.into(),
            block_number: 0.into(),
            block_hash: Hash::default(),
        }
    }

    #[test]
    fn test_compute_tx_dag_and_take_roots() {
        let expected1 = vec![vec![0, 1], vec![2], vec![3], vec![4, 5], vec![6]];
        let transactions1 = vec![
            vec![1],       // (0): dag root
            vec![2],       // (1): dag root
            vec![1, 2, 3], // (2): depends on (0) and (1)
            vec![3, 4, 5], // (3): depends on (2)
            vec![4, 7],    // (4): depends on (3)
            vec![3, 8],    // (5): depends on (3)
            vec![8, 7],    // (6): depends on (4) and (5)
        ];

        let expected2 = vec![vec![0], vec![1, 2], vec![3, 4], vec![5, 6, 7, 8], vec![9]];
        let transactions2 = vec![
            vec![1, 2],           // (0): dag root
            vec![1, 3],           // (1): depends on (0)
            vec![2, 7],           // (2): depends on (0)
            vec![3, 4, 5],        // (3): depends on (1)
            vec![7, 8, 9],        // (4): depends on (2)
            vec![4, 10],          // (5): depends on (3)
            vec![5, 11],          // (6): depends on (3)
            vec![8, 12],          // (7): depends on (4)
            vec![9, 13],          // (8): depends on (4)
            vec![10, 11, 12, 13], // (9): depends on (5), (6), (7) and (8)
        ];

        let expected3 = vec![vec![0, 2, 3], vec![1], vec![4], vec![5, 7], vec![6, 10], vec![8, 11], vec![9]];
        let transactions3 = vec![
            vec![1],                  // (0): dag root
            vec![1, 2, 3],            // (1): depends on (0)
            vec![13],                 // (2): dag root
            vec![14, 15],             // (3): dag root
            vec![2, 4, 5, 6, 13, 14], // (4): depends on (2) and (3)
            vec![4, 12, 15, 16],      // (5): depends on (3) and (4)
            vec![5, 9, 16],           // (6): depends on (4) and (5)
            vec![3, 6, 7, 10],        // (7): depends on (1) and (4),
            vec![9, 10, 11, 12],      // (8): depends on (5), (6) and (7)
            vec![11],                 // (9): depends on (8)
            vec![7, 8],               // (10): depends on (7)
            vec![8],                  // (11): depends on (10)
        ];

        let tests = [transactions1, transactions2, transactions3];
        let expected_results = [expected1, expected2, expected3];

        for (test, expected) in tests.into_iter().zip(expected_results) {
            let transactions = test
                .into_iter()
                .map(|indexes| indexes.into_iter().map(SlotIndex::from))
                .enumerate()
                .map(|(i, indexes)| create_tx(indexes.collect(), i as u64))
                .collect();

            let mut dag = TransactionDag::new(transactions);
            let mut i = 0;
            while let Some(roots) = dag.take_roots() {
                assert_eq!(roots.len(), expected[i].len());
                assert!(roots.iter().all(|tx| expected[i].contains(&tx.transaction_index.inner_value())));
                i += 1;
            }
        }
    }
}