use std::num::TryFromIntError;
use std::ops::Add;
use std::ops::AddAssign;
use std::str::FromStr;

use anyhow::anyhow;
use display_json::DebugAsJson;
use ethereum_types::U64;
use ethers_core::utils::keccak256;
use fake::Dummy;
use fake::Faker;
use sqlx::encode::IsNull;
use sqlx::error::BoxDynError;
use sqlx::postgres::PgHasArrayType;
use sqlx::types::BigDecimal;

use crate::alias::RevmU256;
use crate::eth::primitives::Hash;
use crate::gen_newtype_from;

#[derive(
    DebugAsJson,
    derive_more::Display,
    Clone,
    Copy,
    Default,
    PartialEq,
    Eq,
    PartialOrd,
    Ord,
    Hash,
    derive_more::Add,
    derive_more::Sub,
    serde::Serialize,
    serde::Deserialize,
)]
#[serde(transparent)]
pub struct BlockNumber(pub U64);

impl BlockNumber {
    pub const ZERO: BlockNumber = BlockNumber(U64::zero());
    pub const ONE: BlockNumber = BlockNumber(U64::one());
    pub const MAX: BlockNumber = BlockNumber(U64([i64::MAX as u64])); // use i64 to avoid overflow PostgreSQL because its max limit is i64, not u64.

    /// Calculates the keccak256 hash of the block number.
    pub fn hash(&self) -> Hash {
        Hash::new(keccak256(<[u8; 8]>::from(*self)))
    }

    /// Returns the previous block number.
    pub fn prev(&self) -> Option<Self> {
        if self.is_zero() {
            None
        } else {
            Some(Self(self.0 - 1))
        }
    }

    /// Returns the next block number.
    pub fn next_block_number(&self) -> Self {
        Self(self.0 + 1)
    }

    /// Checks if it is the zero block number.
    pub fn is_zero(&self) -> bool {
        self.0.is_zero()
    }

    /// Count how many blocks there is between itself and the othe block.
    ///
    /// Assumes that self is the lower-end of the range.
    pub fn count_to(self, higher_end: BlockNumber) -> u64 {
        if higher_end >= self {
            higher_end.as_u64() - self.as_u64() + 1
        } else {
            0
        }
    }

    /// Converts itself to i64.
    pub fn as_i64(&self) -> i64 {
        self.0.as_u64() as i64
    }

    /// Converts itself to u64.
    pub fn as_u64(&self) -> u64 {
        self.0.as_u64()
    }
}

impl Dummy<Faker> for BlockNumber {
    fn dummy_with_rng<R: ethers_core::rand::prelude::Rng + ?Sized>(_: &Faker, rng: &mut R) -> Self {
        rng.next_u64().into()
    }
}

// -----------------------------------------------------------------------------
// Math
// -----------------------------------------------------------------------------

impl Add<usize> for BlockNumber {
    type Output = BlockNumber;

    fn add(self, rhs: usize) -> Self::Output {
        Self(self.0 + rhs)
    }
}

impl AddAssign<usize> for BlockNumber {
    fn add_assign(&mut self, rhs: usize) {
        self.0 = self.0 + rhs;
    }
}

// -----------------------------------------------------------------------------
// Conversions: Other -> Self
// -----------------------------------------------------------------------------
gen_newtype_from!(self = BlockNumber, other = u8, u16, u32, u64, U64, usize, i32, i64);

impl FromStr for BlockNumber {
    type Err = anyhow::Error;

    fn from_str(s: &str) -> anyhow::Result<Self> {
        // This parses a hexadecimal string
        match U64::from_str(s) {
            Ok(parsed) => Ok(Self(parsed)),
            Err(e) => {
                tracing::warn!(reason = ?e, value = %s, "failed to parse block number");
                Err(anyhow!("Failed to parse field '{}' with value '{}'", "blockNumber", s))
            }
        }
    }
}

impl TryFrom<BigDecimal> for BlockNumber {
    type Error = anyhow::Error;

    fn try_from(value: BigDecimal) -> Result<Self, Self::Error> {
        let value_str = value.to_string();
        Ok(BlockNumber(U64::from_str_radix(&value_str, 10)?))
    }
}

// -----------------------------------------------------------------------------
// Conversions: Self -> Other
// -----------------------------------------------------------------------------
impl From<BlockNumber> for U64 {
    fn from(block_number: BlockNumber) -> Self {
        block_number.0
    }
}

impl From<BlockNumber> for u64 {
    fn from(block_number: BlockNumber) -> Self {
        block_number.0.as_u64()
    }
}

impl From<BlockNumber> for RevmU256 {
    fn from(block_number: BlockNumber) -> Self {
        Self::from_limbs([block_number.0.as_u64(), 0, 0, 0])
    }
}

impl TryFrom<BlockNumber> for i64 {
    type Error = TryFromIntError;

    fn try_from(block_number: BlockNumber) -> Result<i64, TryFromIntError> {
        i64::try_from(block_number.0.as_u64())
    }
}

impl From<BlockNumber> for [u8; 8] {
    fn from(block_number: BlockNumber) -> Self {
        block_number.0.as_u64().to_be_bytes()
    }
}

// -----------------------------------------------------------------------------
// sqlx traits
// -----------------------------------------------------------------------------
impl<'r> sqlx::Decode<'r, sqlx::Postgres> for BlockNumber {
    fn decode(value: <sqlx::Postgres as sqlx::Database>::ValueRef<'r>) -> Result<Self, BoxDynError> {
        let value = <BigDecimal as sqlx::Decode<sqlx::Postgres>>::decode(value)?;
        Ok(value.try_into()?)
    }
}

impl sqlx::Type<sqlx::Postgres> for BlockNumber {
    fn type_info() -> <sqlx::Postgres as sqlx::Database>::TypeInfo {
        sqlx::postgres::PgTypeInfo::with_name("NUMERIC")
    }
}

impl<'q> sqlx::Encode<'q, sqlx::Postgres> for BlockNumber {
    fn encode_by_ref(&self, buf: &mut <sqlx::Postgres as sqlx::Database>::ArgumentBuffer<'q>) -> Result<IsNull, sqlx::error::BoxDynError> {
        BigDecimal::from(u64::from(*self)).encode(buf)
    }
}

impl PgHasArrayType for BlockNumber {
    fn array_type_info() -> sqlx::postgres::PgTypeInfo {
        <BigDecimal as PgHasArrayType>::array_type_info()
    }
}
