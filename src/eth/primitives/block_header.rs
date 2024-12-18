use display_json::DebugAsJson;
use ethereum_types::H256;
use ethereum_types::H64;
use ethereum_types::U256;
use ethers_core::types::Block as EthersBlock;
use ethers_core::types::OtherFields;
use fake::Dummy;
use fake::Fake;
use fake::Faker;
use hex_literal::hex;
use jsonrpsee::SubscriptionMessage;

use crate::alias::EthersBlockVoid;
use crate::alias::EthersBytes;
use crate::eth::primitives::logs_bloom::LogsBloom;
use crate::eth::primitives::Address;
use crate::eth::primitives::BlockNumber;
use crate::eth::primitives::Bytes;
use crate::eth::primitives::Difficulty;
use crate::eth::primitives::ExternalBlock;
use crate::eth::primitives::Gas;
use crate::eth::primitives::Hash;
use crate::eth::primitives::MinerNonce;
use crate::eth::primitives::Size;
use crate::eth::primitives::UnixTime;
use crate::ext::InfallibleExt;

/// Special hash used in block mining to indicate no uncle blocks.
const HASH_EMPTY_UNCLES: Hash = Hash::new(hex!("1dcc4de8dec75d7aab85b567b6ccd41ad312451b948a7413f0a142fd40d49347"));

/// Special hash used in block mining to indicate no transaction root and no receipts root.
const HASH_EMPTY_TRIE: Hash = Hash::new(hex!("56e81f171bcc55a6ff8345e692c0f86e5b48e01b996cadc001622fb5e363b421"));

#[derive(DebugAsJson, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct BlockHeader {
    pub number: BlockNumber,
    pub hash: Hash,
    pub transactions_root: Hash,
    pub gas_used: Gas,
    pub gas_limit: Gas,
    pub bloom: LogsBloom,
    pub timestamp: UnixTime,
    pub parent_hash: Hash,
    pub author: Address,
    pub extra_data: Bytes,
    pub miner: Address,
    pub difficulty: Difficulty, // is always 0x0
    pub receipts_root: Hash,
    pub uncle_hash: Hash, // is always 0x1dcc4de8dec75d7aab85b567b6ccd41ad312451b948a7413f0a142fd40d49347
    pub size: Size,
    pub state_root: Hash,
    pub total_difficulty: Difficulty, // is always 0x0
    pub nonce: MinerNonce,            // is always 0x0000000000000000
}

impl BlockHeader {
    /// Creates a new block header with the given number.
    pub fn new(number: BlockNumber, timestamp: UnixTime) -> Self {
        Self {
            number,
            hash: number.hash(),
            transactions_root: HASH_EMPTY_TRIE,
            gas_used: Gas::ZERO,
            gas_limit: Gas::ZERO,
            bloom: LogsBloom::default(),
            timestamp,
            parent_hash: number.prev().map(|n| n.hash()).unwrap_or(Hash::ZERO),
            author: Address::default(),
            extra_data: Bytes::default(),
            miner: Address::default(),
            difficulty: Difficulty::default(),
            receipts_root: HASH_EMPTY_TRIE,
            uncle_hash: HASH_EMPTY_UNCLES,
            size: Size::default(),
            state_root: HASH_EMPTY_TRIE,
            total_difficulty: Difficulty::default(),
            nonce: MinerNonce::default(),
        }
    }
}

impl Dummy<Faker> for BlockHeader {
    fn dummy_with_rng<R: ethers_core::rand::prelude::Rng + ?Sized>(faker: &Faker, rng: &mut R) -> Self {
        Self {
            number: faker.fake_with_rng(rng),
            hash: faker.fake_with_rng(rng),
            transactions_root: faker.fake_with_rng(rng),
            gas_used: faker.fake_with_rng(rng),
            bloom: LogsBloom::default(),
            timestamp: faker.fake_with_rng(rng),
            parent_hash: faker.fake_with_rng(rng),
            gas_limit: faker.fake_with_rng(rng),
            author: faker.fake_with_rng(rng),
            extra_data: faker.fake_with_rng(rng),
            miner: faker.fake_with_rng(rng),
            difficulty: faker.fake_with_rng(rng),
            receipts_root: faker.fake_with_rng(rng),
            uncle_hash: faker.fake_with_rng(rng),
            size: faker.fake_with_rng(rng),
            state_root: faker.fake_with_rng(rng),
            total_difficulty: faker.fake_with_rng(rng),
            nonce: faker.fake_with_rng(rng),
        }
    }
}

// -----------------------------------------------------------------------------
// Conversions: Self -> Other
// -----------------------------------------------------------------------------
impl<T> From<BlockHeader> for EthersBlock<T>
where
    T: Default,
{
    fn from(header: BlockHeader) -> Self {
        Self {
            // block: identifiers
            hash: Some(header.hash.into()),
            number: Some(header.number.into()),
            mix_hash: Some(H256::default()),

            // block: relation with other blocks
            uncles_hash: HASH_EMPTY_UNCLES.into(),
            uncles: Vec::new(),
            parent_beacon_block_root: None,
            parent_hash: header.parent_hash.into(),

            // mining: identifiers
            timestamp: (*header.timestamp).into(),
            author: Some(Address::COINBASE.into()),

            // minining: difficulty
            difficulty: U256::zero(),
            total_difficulty: Some(U256::zero()),
            nonce: Some(H64::zero()),

            // mining: gas
            gas_limit: Gas::from(100_000_000u64).into(),
            gas_used: header.gas_used.into(),
            base_fee_per_gas: Some(U256::zero()),
            blob_gas_used: None,
            excess_blob_gas: None,

            // transactions
            transactions: vec![], // can't fill transactions from header, must be modified afterward
            transactions_root: header.transactions_root.into(),
            receipts_root: HASH_EMPTY_TRIE.into(),
            withdrawals_root: None,
            withdrawals: None,

            // data
            size: Some(u64::from(header.size).into()),
            logs_bloom: Some(*header.bloom),
            extra_data: EthersBytes::default(),
            state_root: header.state_root.into(),
            seal_fields: Vec::default(),
            other: OtherFields::default(),
        }
    }
}

// -----------------------------------------------------------------------------
// Conversions: Other -> Self
// -----------------------------------------------------------------------------

impl TryFrom<&ExternalBlock> for BlockHeader {
    type Error = anyhow::Error;
    fn try_from(value: &ExternalBlock) -> Result<Self, Self::Error> {
        Ok(Self {
            number: value.number(),
            hash: value.hash(),
            transactions_root: value.transactions_root.into(),
            gas_used: value.gas_used.try_into()?,
            bloom: value.logs_bloom.unwrap_or_default().into(),
            timestamp: value.timestamp.into(),
            parent_hash: value.parent_hash.into(),
            gas_limit: value.gas_limit.try_into()?,
            author: value.author(),
            extra_data: value.extra_data.clone().into(),
            miner: value.author.unwrap_or_default().into(),
            difficulty: value.difficulty.into(),
            receipts_root: value.receipts_root.into(),
            uncle_hash: value.uncles_hash.into(),
            size: value.size.unwrap_or_default().try_into()?,
            state_root: value.state_root.into(),
            total_difficulty: value.total_difficulty.unwrap_or_default().into(),
            nonce: value.nonce.unwrap_or_default().into(),
        })
    }
}

impl From<BlockHeader> for SubscriptionMessage {
    fn from(value: BlockHeader) -> Self {
        let ethers_block = EthersBlockVoid::from(value);
        Self::from_json(&ethers_block).expect_infallible()
    }
}

// -----------------------------------------------------------------------------
// Tests
// -----------------------------------------------------------------------------
#[cfg(test)]
mod tests {
    use crate::eth::primitives::BlockHeader;
    use crate::eth::primitives::BlockNumber;
    use crate::eth::primitives::Hash;
    use crate::eth::primitives::UnixTime;

    #[test]
    fn block_header_hash_calculation() {
        let header = BlockHeader::new(BlockNumber::ZERO, UnixTime::from(1234567890));
        assert_eq!(header.hash.to_string(), "0x011b4d03dd8c01f1049143cf9c4c817e4b167f1d1b83e5c6f0f10d89ba1e7bce");
    }

    #[test]
    fn block_header_parent_hash() {
        let header = BlockHeader::new(BlockNumber::ONE, UnixTime::from(1234567891));
        assert_eq!(
            header.parent_hash.to_string(),
            "0x011b4d03dd8c01f1049143cf9c4c817e4b167f1d1b83e5c6f0f10d89ba1e7bce"
        );
    }

    #[test]
    fn block_header_genesis_parent_hash() {
        let header = BlockHeader::new(BlockNumber::ZERO, UnixTime::from(1234567890));
        assert_eq!(header.parent_hash, Hash::ZERO);
    }
}
