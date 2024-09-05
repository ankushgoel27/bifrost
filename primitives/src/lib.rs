// This file is part of Bifrost.

// Copyright (C) Liebi Technologies PTE. LTD.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0

// This program is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.

// This program is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE. See the
// GNU General Public License for more details.

// You should have received a copy of the GNU General Public License
// along with this program. If not, see <https://www.gnu.org/licenses/>.

//! Low-level types used throughout the Bifrost code.

#![cfg_attr(not(feature = "std"), no_std)]

use orml_traits::{xcm_transfer::Transferred, XcmTransfer};
use parity_scale_codec::MaxEncodedLen;
use scale_info::TypeInfo;
use sp_core::{ConstU32, Decode, Encode, RuntimeDebug, H160};
use sp_runtime::{
	generic,
	traits::{BlakeTwo256, IdentifyAccount, Verify},
	DispatchError, FixedU128, MultiSignature, OpaqueExtrinsic, Permill,
};
use sp_std::vec::Vec;
use xcm::v4::{prelude::*, Asset, Location};
use xcm_executor::traits::{AssetTransferError, TransferType, XcmAssetTransfers};

pub mod currency;
pub use currency::*;
mod salp;
pub mod traits;
pub use salp::*;

#[cfg(test)]
mod tests;

pub use crate::traits::*;

/// An index to a block.
pub type BlockNumber = u32;

/// Alias to 512-bit hash when used in the context of a transaction signature on the chain.
pub type Signature = MultiSignature;

/// Some way of identifying an account on the chain. We intentionally make it equivalent
/// to the public key of our transaction signing scheme.
pub type AccountId = <<Signature as Verify>::Signer as IdentifyAccount>::AccountId;

/// The type for looking up accounts. We don't expect more than 4 billion of them.
pub type AccountIndex = u32;

/// An index to an asset
pub type AssetId = u32;

/// Vtoken Mint type
pub type VtokenMintPrice = u128;

/// Balance of an account.
pub type Balance = u128;

/// Price of an asset.
pub type Price = FixedU128;

pub type PriceDetail = (Price, Timestamp);

/// Precision of symbol.
pub type Precision = u32;

/// Type used for expressing timestamp.
pub type Moment = u64;

/// Index of a transaction in the chain.
pub type Index = u32;

/// A hash of some data used by the chain.
pub type Hash = sp_core::H256;

/// A timestamp: milliseconds since the unix epoch.
/// `u64` is enough to represent a duration of half a billion years, when the
/// time scale is milliseconds.
pub type Timestamp = u64;

/// Digest item type.
pub type DigestItem = generic::DigestItem;

/// Header type.
pub type Header = generic::Header<BlockNumber, BlakeTwo256>;

/// Block type.
pub type Block = generic::Block<Header, OpaqueExtrinsic>;

/// Block ID.
pub type BlockId = generic::BlockId<Block>;

/// Balancer pool swap fee.
pub type SwapFee = u128;

/// Balancer pool ID.
pub type PoolId = u32;

/// Balancer pool weight.
pub type PoolWeight = u128;

/// Balancer pool token.
pub type PoolToken = u128;

/// Index of a transaction in the chain. 32-bit should be plenty.
pub type Nonce = u32;

///
pub type BiddingOrderId = u64;

///
pub type EraId = u32;

/// Signed version of Balance
pub type Amount = i128;

/// Parachain Id
pub type ParaId = u32;

/// The measurement type for counting lease periods (generally the same as `BlockNumber`).
pub type LeasePeriod = BlockNumber;

/// Index used for the child trie
pub type TrieIndex = u32;

/// Distribution Id
pub type DistributionId = u32;

/// The fixed point number
pub type Rate = FixedU128;

/// The fixed point number, range from 0 to 1.
pub type Ratio = Permill;

pub type Liquidity = FixedU128;

pub type Shortfall = FixedU128;

pub const SECONDS_PER_YEAR: Timestamp = 365 * 24 * 60 * 60;

pub type DerivativeIndex = u16;

pub type TimeStampedPrice = orml_oracle::TimestampedValue<Price, Moment>;

pub type AstarParachainId = ConstU32<2006>;

pub type MoonbeamParachainId = ConstU32<2004>;

pub type HydradxParachainId = ConstU32<2034>;

pub type MantaParachainId = ConstU32<2104>;

pub type InterlayParachainId = ConstU32<2032>;

#[derive(
	Encode, Decode, Eq, PartialEq, Copy, Clone, RuntimeDebug, PartialOrd, Ord, scale_info::TypeInfo,
)]
pub enum ExtraFeeName {
	SalpContribute,
	StatemineTransfer,
	VoteVtoken,
	VoteRemoveDelegatorVote,
	NoExtraFee,
	EthereumTransfer,
}

// For vtoken-minting and slp modules
#[derive(Encode, Decode, Clone, RuntimeDebug, Eq, TypeInfo, MaxEncodedLen)]
pub enum TimeUnit {
	// Kusama staking time unit
	Era(#[codec(compact)] u32),
	SlashingSpan(#[codec(compact)] u32),
	// Moonriver staking time unit
	Round(#[codec(compact)] u32),
	// 1000 blocks. Can be used by Filecoin.
	// 30 seconds per block. Kblock means 8.33 hours.
	Kblock(#[codec(compact)] u32),
	// 1 hour. Should be Unix Timstamp in seconds / 3600
	Hour(#[codec(compact)] u32),
}

impl TimeUnit {
	pub fn add_one(self) -> Self {
		match self {
			TimeUnit::Era(a) => TimeUnit::Era(a.saturating_add(1)),
			TimeUnit::SlashingSpan(a) => TimeUnit::SlashingSpan(a.saturating_add(1)),
			TimeUnit::Round(a) => TimeUnit::Round(a.saturating_add(1)),
			TimeUnit::Kblock(a) => TimeUnit::Kblock(a.saturating_add(1)),
			TimeUnit::Hour(a) => TimeUnit::Hour(a.saturating_add(1)),
		}
	}

	pub fn add(self, other_time: Self) -> Option<Self> {
		match (self, other_time) {
			(TimeUnit::Era(a), TimeUnit::Era(b)) => Some(TimeUnit::Era(a.saturating_add(b))),
			_ => None,
		}
	}
}

impl Default for TimeUnit {
	fn default() -> Self {
		TimeUnit::Era(0u32)
	}
}

impl PartialEq for TimeUnit {
	fn eq(&self, other: &Self) -> bool {
		match (&self, other) {
			(Self::Era(a), Self::Era(b)) => a.eq(b),
			(Self::SlashingSpan(a), Self::SlashingSpan(b)) => a.eq(b),
			(Self::Round(a), Self::Round(b)) => a.eq(b),
			(Self::Kblock(a), Self::Kblock(b)) => a.eq(b),
			(Self::Hour(a), Self::Hour(b)) => a.eq(b),
			_ => false,
		}
	}
}

impl Ord for TimeUnit {
	fn cmp(&self, other: &Self) -> sp_std::cmp::Ordering {
		match (&self, other) {
			(Self::Era(a), Self::Era(b)) => a.cmp(b),
			(Self::SlashingSpan(a), Self::SlashingSpan(b)) => a.cmp(b),
			(Self::Round(a), Self::Round(b)) => a.cmp(b),
			(Self::Kblock(a), Self::Kblock(b)) => a.cmp(b),
			(Self::Hour(a), Self::Hour(b)) => a.cmp(b),
			_ => sp_std::cmp::Ordering::Less,
		}
	}
}

impl PartialOrd for TimeUnit {
	fn partial_cmp(&self, other: &Self) -> Option<sp_std::cmp::Ordering> {
		match (&self, other) {
			(Self::Era(a), Self::Era(b)) => Some(a.cmp(b)),
			(Self::SlashingSpan(a), Self::SlashingSpan(b)) => Some(a.cmp(b)),
			(Self::Round(a), Self::Round(b)) => Some(a.cmp(b)),
			(Self::Kblock(a), Self::Kblock(b)) => Some(a.cmp(b)),
			(Self::Hour(a), Self::Hour(b)) => Some(a.cmp(b)),
			_ => None,
		}
	}
}

// For vtoken-minting
#[derive(
	PartialEq, Eq, Clone, Encode, Decode, MaxEncodedLen, RuntimeDebug, scale_info::TypeInfo,
)]
pub enum RedeemType<AccountId> {
	/// Native chain.
	Native,
	/// Astar chain.
	Astar(AccountId),
	/// Moonbeam chain.
	Moonbeam(H160),
	/// Hydradx chain.
	Hydradx(AccountId),
	/// Interlay chain.
	Interlay(AccountId),
	/// Manta chain.
	Manta(AccountId),
}

impl<AccountId> Default for RedeemType<AccountId> {
	fn default() -> Self {
		Self::Native
	}
}

pub struct DoNothingRouter;
impl SendXcm for DoNothingRouter {
	type Ticket = ();
	fn validate(_dest: &mut Option<Location>, _msg: &mut Option<Xcm<()>>) -> SendResult<()> {
		Ok(((), Assets::new()))
	}
	fn deliver(_: ()) -> Result<XcmHash, SendError> {
		Ok([0; 32])
	}
}

pub struct MockXcmTransfer;
impl XcmTransfer<AccountId, Balance, CurrencyId> for MockXcmTransfer {
	fn transfer(
		who: AccountId,
		_currency_id: CurrencyId,
		amount: Balance,
		dest: Location,
		_dest_weight_limit: WeightLimit,
	) -> Result<Transferred<AccountId>, DispatchError> {
		Ok(Transferred {
			sender: who,
			assets: Default::default(),
			fee: Asset { id: AssetId(Location::here()), fun: Fungible(amount) },
			dest,
		})
	}

	fn transfer_multiasset(
		_who: AccountId,
		_asset: Asset,
		_dest: Location,
		_dest_weight_limit: WeightLimit,
	) -> Result<Transferred<AccountId>, DispatchError> {
		unimplemented!()
	}

	fn transfer_with_fee(
		_who: AccountId,
		_currency_id: CurrencyId,
		_amount: Balance,
		_fee: Balance,
		_dest: Location,
		_dest_weight_limit: WeightLimit,
	) -> Result<Transferred<AccountId>, DispatchError> {
		unimplemented!()
	}

	fn transfer_multiasset_with_fee(
		_who: AccountId,
		_asset: Asset,
		_fee: Asset,
		_dest: Location,
		_dest_weight_limit: WeightLimit,
	) -> Result<Transferred<AccountId>, DispatchError> {
		unimplemented!()
	}

	fn transfer_multicurrencies(
		_who: AccountId,
		_currencies: Vec<(CurrencyId, Balance)>,
		_fee_item: u32,
		_dest: Location,
		_dest_weight_limit: WeightLimit,
	) -> Result<Transferred<AccountId>, DispatchError> {
		unimplemented!()
	}

	fn transfer_multiassets(
		_who: AccountId,
		_assets: Assets,
		_fee: Asset,
		_dest: Location,
		_dest_weight_limit: WeightLimit,
	) -> Result<Transferred<AccountId>, DispatchError> {
		unimplemented!()
	}
}

pub struct Weightless;
impl PreparedMessage for Weightless {
	fn weight_of(&self) -> Weight {
		Weight::default()
	}
}

pub struct DoNothingExecuteXcm;
impl<Call> ExecuteXcm<Call> for DoNothingExecuteXcm {
	type Prepared = Weightless;

	fn prepare(_message: Xcm<Call>) -> Result<Self::Prepared, Xcm<Call>> {
		Ok(Weightless)
	}

	fn execute(
		_origin: impl Into<Location>,
		_pre: Self::Prepared,
		_hash: &mut XcmHash,
		_weight_credit: Weight,
	) -> Outcome {
		Outcome::Complete { used: Weight::default() }
	}

	fn charge_fees(_location: impl Into<Location>, _fees: Assets) -> XcmResult {
		Ok(())
	}
}

impl XcmAssetTransfers for DoNothingExecuteXcm {
	type IsReserve = ();
	type IsTeleporter = ();
	type AssetTransactor = ();

	fn determine_for(_asset: &Asset, _dest: &Location) -> Result<TransferType, AssetTransferError> {
		Ok(TransferType::DestinationReserve)
	}
}

#[derive(Encode, Decode, Eq, PartialEq, Copy, Clone, RuntimeDebug, TypeInfo)]
pub enum XcmOperationType {
	// SALP operations
	UmpContributeTransact,
	// Statemine operations
	StatemineTransfer,
	// SLP operations
	Bond,
	WithdrawUnbonded,
	BondExtra,
	Unbond,
	Rebond,
	Delegate,
	Payout,
	Liquidize,
	TransferBack,
	TransferTo,
	Chill,
	Undelegate,
	CancelLeave,
	XtokensTransferBack,
	ExecuteLeave,
	ConvertAsset,
	// VtokenVoting operations
	Vote,
	RemoveVote,
	Any,
	SupplementaryFee,
	EthereumTransfer,
	TeleportAssets,
}

pub struct ExtraFeeInfo {
	pub extra_fee_name: ExtraFeeName,
	pub extra_fee_currency: CurrencyId,
}

impl Default for ExtraFeeInfo {
	fn default() -> Self {
		Self {
			extra_fee_name: ExtraFeeName::NoExtraFee,
			extra_fee_currency: CurrencyId::Native(TokenSymbol::BNC),
		}
	}
}
