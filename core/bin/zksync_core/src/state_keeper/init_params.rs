use std::collections::HashMap;
// External uses
// Workspace uses
use zksync_types::{
    block::PendingBlock as SendablePendingBlock, Account, AccountId, AccountTree, Address,
    BlockNumber, TokenId, NFT,
};

use super::{
    root_hash_calculator::BlockRootHashJob,
    state_restore::{db::StateRestoreStorage, RestoredTree},
};

#[derive(Debug)]
pub struct ZkSyncStateInitParams {
    pub tree: AccountTree,
    pub acc_id_by_addr: HashMap<Address, AccountId>,
    pub nfts: HashMap<TokenId, NFT>,
    pub last_block_number: BlockNumber,
    pub unprocessed_priority_op: u64,

    pub pending_block: Option<SendablePendingBlock>,
    pub root_hash_jobs: Vec<BlockRootHashJob>,
}

impl Default for ZkSyncStateInitParams {
    fn default() -> Self {
        Self::new()
    }
}

impl ZkSyncStateInitParams {
    pub fn new() -> Self {
        Self {
            tree: AccountTree::new(zksync_crypto::params::account_tree_depth()),
            acc_id_by_addr: HashMap::new(),
            nfts: HashMap::new(),
            last_block_number: BlockNumber(0),
            unprocessed_priority_op: 0,

            pending_block: None,
            root_hash_jobs: Vec::new(),
        }
    }

    pub async fn restore_from_db(
        storage: &mut zksync_storage::StorageProcessor<'_>,
    ) -> Result<Self, anyhow::Error> {
        let mut init_params = Self::new();
        init_params.load_from_db(storage).await?;

        Ok(init_params)
    }

    async fn load_account_tree(
        &mut self,
        storage: &mut zksync_storage::StorageProcessor<'_>,
    ) -> BlockNumber {
        let mut restored_tree = RestoredTree::new(StateRestoreStorage::new(storage));
        let last_block_number = restored_tree.restore().await;
        self.tree = restored_tree.tree;
        self.acc_id_by_addr = restored_tree.acc_id_by_addr;
        last_block_number
    }

    async fn load_from_db(
        &mut self,
        storage: &mut zksync_storage::StorageProcessor<'_>,
    ) -> Result<(), anyhow::Error> {
        let block_number = self.load_account_tree(storage).await;
        self.last_block_number = block_number;

        self.unprocessed_priority_op =
            Self::unprocessed_priority_op_id(storage, block_number).await?;
        self.nfts = Self::load_nft_tokens(storage, block_number).await?;

        self.pending_block = Self::load_pending_block(storage, block_number).await;
        self.root_hash_jobs = Self::load_root_hash_jobs(storage).await;

        vlog::info!(
            "Loaded committed state: last block number: {}, unprocessed priority op: {}",
            *self.last_block_number,
            self.unprocessed_priority_op
        );
        Ok(())
    }

    async fn load_pending_block(
        storage: &mut zksync_storage::StorageProcessor<'_>,
        last_block_number: BlockNumber,
    ) -> Option<SendablePendingBlock> {
        let pending_block = storage
            .chain()
            .block_schema()
            .load_pending_block()
            .await
            .unwrap_or_default()?;

        if pending_block.number <= last_block_number {
            // If after generating several pending block node generated
            // full blocks, they may be sealed on the first iteration
            // and stored pending block will be outdated.
            // Thus, if the stored pending block has the lower number than
            // last committed one, we just ignore it.
            return None;
        }

        // We've checked that pending block is greater than the last committed block,
        // but it must be greater exactly by 1.
        assert_eq!(*pending_block.number, *last_block_number + 1);

        Some(pending_block)
    }

    async fn load_root_hash_jobs(
        storage: &mut zksync_storage::StorageProcessor<'_>,
    ) -> Vec<BlockRootHashJob> {
        if let Some((block_from, block_to)) = storage
            .chain()
            .block_schema()
            .incomplete_blocks_range()
            .await
            .expect("Unable to load incomplete blocks range")
        {
            let mut state_schema = storage.chain().state_schema();

            let mut jobs = Vec::with_capacity((block_to.0 - block_from.0 + 1) as usize);

            for block in (block_from.0..=block_to.0).map(BlockNumber) {
                let updates = state_schema
                    .load_state_diff_for_block(block)
                    .await
                    .unwrap_or_else(|err| {
                        panic!("Unable to load state updates for block {}: {}", block, err)
                    });

                jobs.push(BlockRootHashJob { block, updates })
            }

            jobs
        } else {
            Vec::new()
        }
    }

    pub fn insert_account(&mut self, id: AccountId, acc: Account) {
        self.acc_id_by_addr.insert(acc.address, id);
        self.tree.insert(*id, acc);
    }

    async fn load_nft_tokens(
        storage: &mut zksync_storage::StorageProcessor<'_>,
        block_number: BlockNumber,
    ) -> anyhow::Result<HashMap<TokenId, NFT>> {
        let nfts = storage
            .chain()
            .state_schema()
            .load_committed_nft_tokens(Some(block_number))
            .await?
            .into_iter()
            .map(|nft| {
                let token: NFT = nft.into();
                (token.id, token)
            })
            .collect();
        Ok(nfts)
    }

    async fn unprocessed_priority_op_id(
        storage: &mut zksync_storage::StorageProcessor<'_>,
        block_number: BlockNumber,
    ) -> Result<u64, anyhow::Error> {
        let block = storage
            .chain()
            .block_schema()
            .get_block(block_number)
            .await?;

        if let Some(block) = block {
            Ok(block.processed_priority_ops.1)
        } else {
            Ok(0)
        }
    }
}
