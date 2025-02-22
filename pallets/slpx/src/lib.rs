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

#![cfg_attr(not(feature = "std"), no_std)]
use crate::types::{
	AccountIdOf, BalanceOf, CurrencyIdOf, EthereumCallConfiguration, EthereumXcmCall,
	EthereumXcmTransaction, EthereumXcmTransactionV2, MoonbeamCall, Order, OrderCaller, OrderType,
	SupportChain, TargetChain, EVM_FUNCTION_SELECTOR, MAX_GAS_LIMIT,
};
use bifrost_asset_registry::AssetMetadata;
use bifrost_primitives::{
	currency::{BNC, VFIL},
	AstarChainId, CurrencyId, CurrencyIdMapping, HydrationChainId, InterlayChainId, MantaChainId,
	RedeemType, SlpxOperator, TokenInfo, VtokenMintingInterface,
};
use cumulus_primitives_core::ParaId;
use ethereum::TransactionAction;
use frame_support::{
	dispatch::{DispatchResult, DispatchResultWithPostInfo},
	ensure,
	pallet_prelude::ConstU32,
	sp_runtime::SaturatedConversion,
	traits::Get,
	transactional,
};
use frame_system::{
	ensure_signed,
	pallet_prelude::{BlockNumberFor, OriginFor},
};
use orml_traits::{MultiCurrency, XcmTransfer};
pub use pallet::*;
use parity_scale_codec::{Decode, Encode};
use polkadot_parachain_primitives::primitives::{Id, Sibling};
use sp_core::{Hasher, H160, U256};
use sp_runtime::{
	traits::{AccountIdConversion, BlakeTwo256, CheckedSub, UniqueSaturatedFrom},
	BoundedVec, DispatchError,
};
use sp_std::{vec, vec::Vec};
use xcm::v4::{prelude::*, Location};
use zenlink_protocol::AssetBalance;

pub mod migration;
pub mod types;

pub mod weights;
pub use weights::WeightInfo;

#[cfg(test)]
mod mock;

#[cfg(test)]
mod tests;

#[cfg(feature = "runtime-benchmarks")]
mod benchmarking;

#[frame_support::pallet]
pub mod pallet {
	use super::*;
	use crate::types::{Order, OrderType};
	use bifrost_primitives::{currency::MOVR, BifrostKusamaChainId, GLMR};
	use bifrost_stable_pool::{traits::StablePoolHandler, PoolTokenIndex, StableAssetPoolId};
	use frame_support::{
		pallet_prelude::{ValueQuery, *},
		weights::WeightMeter,
	};
	use frame_system::ensure_root;
	use zenlink_protocol::{AssetId, ExportZenlink};

	const STORAGE_VERSION: StorageVersion = StorageVersion::new(1);

	#[pallet::pallet]
	#[pallet::storage_version(STORAGE_VERSION)]
	pub struct Pallet<T>(_);

	#[pallet::config]
	pub trait Config: frame_system::Config {
		type RuntimeEvent: From<Event<Self>> + IsType<<Self as frame_system::Config>::RuntimeEvent>;
		type ControlOrigin: EnsureOrigin<<Self as frame_system::Config>::RuntimeOrigin>;
		type MultiCurrency: MultiCurrency<AccountIdOf<Self>, CurrencyId = CurrencyId>;

		type DexOperator: ExportZenlink<Self::AccountId, AssetId>;

		/// The interface to call VtokenMinting module functions.
		type VtokenMintingInterface: VtokenMintingInterface<
			AccountIdOf<Self>,
			CurrencyIdOf<Self>,
			BalanceOf<Self>,
		>;

		/// The interface to call StablePool module functions.
		type StablePoolHandler: StablePoolHandler<
			Balance = BalanceOf<Self>,
			AccountId = AccountIdOf<Self>,
			CurrencyId = CurrencyIdOf<Self>,
		>;

		/// xtokens xcm transfer interface
		type XcmTransfer: XcmTransfer<AccountIdOf<Self>, BalanceOf<Self>, CurrencyIdOf<Self>>;

		///
		type XcmSender: SendXcm;

		/// Convert Location to `T::CurrencyId`.
		type CurrencyIdConvert: CurrencyIdMapping<
			CurrencyId,
			xcm::v3::MultiLocation,
			AssetMetadata<BalanceOf<Self>>,
		>;

		/// TreasuryAccount
		#[pallet::constant]
		type TreasuryAccount: Get<AccountIdOf<Self>>;

		#[pallet::constant]
		type ParachainId: Get<ParaId>;

		type WeightInfo: WeightInfo;
	}

	#[pallet::event]
	#[pallet::generate_deposit(pub(super) fn deposit_event)]
	pub enum Event<T: Config> {
		AddWhitelistAccountId {
			support_chain: SupportChain,
			evm_contract_account_id: AccountIdOf<T>,
		},
		RemoveWhitelistAccountId {
			support_chain: SupportChain,
			evm_contract_account_id: AccountIdOf<T>,
		},
		XcmMint {
			evm_caller: H160,
			currency_id: CurrencyIdOf<T>,
			token_amount: BalanceOf<T>,
			target_chain: TargetChain<AccountIdOf<T>>,
		},
		XcmMintFailed {
			evm_caller: H160,
			currency_id: CurrencyIdOf<T>,
			token_amount: BalanceOf<T>,
			target_chain: TargetChain<AccountIdOf<T>>,
		},
		XcmZenlinkSwap {
			evm_caller: H160,
			currency_id_in: CurrencyIdOf<T>,
			currency_id_out: CurrencyIdOf<T>,
			currency_id_out_amount: BalanceOf<T>,
			target_chain: TargetChain<AccountIdOf<T>>,
		},
		XcmZenlinkSwapFailed {
			evm_caller: H160,
			currency_id_in: CurrencyIdOf<T>,
			currency_id_out: CurrencyIdOf<T>,
			currency_id_in_amount: BalanceOf<T>,
			target_chain: TargetChain<AccountIdOf<T>>,
		},
		XcmStablePoolSwap {
			evm_caller: H160,
			pool_token_index_in: PoolTokenIndex,
			pool_token_index_out: PoolTokenIndex,
			currency_id_out_amount: BalanceOf<T>,
			target_chain: TargetChain<AccountIdOf<T>>,
		},
		XcmStablePoolSwapFailed {
			evm_caller: H160,
			pool_token_index_in: PoolTokenIndex,
			pool_token_index_out: PoolTokenIndex,
			currency_id_in_amount: BalanceOf<T>,
			target_chain: TargetChain<AccountIdOf<T>>,
		},
		XcmRedeem {
			evm_caller: H160,
			vtoken_id: CurrencyIdOf<T>,
			vtoken_amount: BalanceOf<T>,
			target_chain: TargetChain<AccountIdOf<T>>,
		},
		XcmRedeemFailed {
			evm_caller: H160,
			vtoken_id: CurrencyIdOf<T>,
			vtoken_amount: BalanceOf<T>,
			target_chain: TargetChain<AccountIdOf<T>>,
		},
		SetTransferToFee {
			support_chain: SupportChain,
			transfer_to_fee: BalanceOf<T>,
		},
		SetExecutionFee {
			currency_id: CurrencyId,
			execution_fee: BalanceOf<T>,
		},
		SetCurrencyEthereumCallSwitch {
			currency_id: CurrencyId,
			is_support: bool,
		},
		SetEthereumCallConfiguration {
			xcm_fee: u128,
			xcm_weight: Weight,
			period: BlockNumberFor<T>,
			contract: H160,
		},
		XcmSetTokenAmount {
			currency_id: CurrencyId,
			token_amount: BalanceOf<T>,
			vcurrency_id: CurrencyId,
			vtoken_amount: BalanceOf<T>,
		},
		SetCurrencyToSupportXcmFee {
			currency_id: CurrencyId,
			is_support: bool,
		},
		SetDelayBlock {
			delay_block: BlockNumberFor<T>,
		},
		CreateOrder {
			order: Order<AccountIdOf<T>, CurrencyIdOf<T>, BalanceOf<T>, BlockNumberFor<T>>,
		},
		OrderHandled {
			order: Order<AccountIdOf<T>, CurrencyIdOf<T>, BalanceOf<T>, BlockNumberFor<T>>,
		},
		OrderFailed {
			order: Order<AccountIdOf<T>, CurrencyIdOf<T>, BalanceOf<T>, BlockNumberFor<T>>,
		},
		InsufficientAssets,
	}

	#[pallet::error]
	pub enum Error<T> {
		/// Token not found in vtoken minting
		TokenNotFoundInVtokenMinting,
		/// Token not found in zenlink
		TokenNotFoundInZenlink,
		/// Contract Account already exists in the whitelist
		AccountIdAlreadyInWhitelist,
		/// Contract Account is not in the whitelist
		AccountIdNotInWhitelist,
		/// The maximum number of whitelist addresses is 10
		ExceededWhitelistMaxNumber,
		/// Execution fee not set
		NotSetExecutionFee,
		/// Insufficient balance to execute the fee
		FreeBalanceTooLow,
		/// ArgumentsError
		ArgumentsError,
		ErrorConvertVtoken,
		ErrorValidating,
		ErrorDelivering,
		Unsupported,
	}

	/// Contract whitelist
	#[pallet::storage]
	pub type WhitelistAccountId<T: Config> = StorageMap<
		_,
		Blake2_128Concat,
		SupportChain,
		BoundedVec<AccountIdOf<T>, ConstU32<10>>,
		ValueQuery,
	>;

	/// Charge corresponding fees for different CurrencyId
	#[pallet::storage]
	pub type ExecutionFee<T: Config> =
		StorageMap<_, Blake2_128Concat, CurrencyId, BalanceOf<T>, OptionQuery>;

	/// XCM fee for transferring to Moonbeam(BNC)
	#[pallet::storage]
	pub type TransferToFee<T: Config> =
		StorageMap<_, Blake2_128Concat, SupportChain, BalanceOf<T>, OptionQuery>;

	#[pallet::storage]
	pub type XcmEthereumCallConfiguration<T: Config> =
		StorageValue<_, EthereumCallConfiguration<BlockNumberFor<T>>>;

	#[pallet::storage]
	pub type CurrencyIdList<T: Config> =
		StorageValue<_, BoundedVec<CurrencyId, ConstU32<10>>, ValueQuery>;

	#[pallet::storage]
	pub type SupportXcmFeeList<T: Config> =
		StorageValue<_, BoundedVec<CurrencyId, ConstU32<100>>, ValueQuery>;

	#[pallet::storage]
	pub type OrderQueue<T: Config> = StorageValue<
		_,
		BoundedVec<
			Order<AccountIdOf<T>, CurrencyIdOf<T>, BalanceOf<T>, BlockNumberFor<T>>,
			ConstU32<1000>,
		>,
		ValueQuery,
	>;

	#[pallet::storage]
	pub type DelayBlock<T: Config> = StorageValue<_, BlockNumberFor<T>, ValueQuery>;

	#[pallet::hooks]
	impl<T: Config> Hooks<BlockNumberFor<T>> for Pallet<T> {
		fn on_idle(n: BlockNumberFor<T>, limit: Weight) -> Weight {
			let mut weight = Weight::default();

			if WeightMeter::with_limit(limit)
				.try_consume(T::DbWeight::get().reads_writes(4, 2))
				.is_err()
			{
				return weight;
			}

			let mut is_handle_xcm_oracle = false;

			let mut currency_list = CurrencyIdList::<T>::get().to_vec();
			if currency_list.len() < 1 {
				weight = weight.saturating_add(T::DbWeight::get().reads_writes(1, 0));
			} else {
				let configuration = XcmEthereumCallConfiguration::<T>::get();
				match configuration {
					Some(mut configuration) => {
						let currency_id = currency_list[0];
						let token_amount = T::VtokenMintingInterface::get_token_pool(currency_id);
						// It's impossible to go wrong.
						let vcurrency_id = T::VtokenMintingInterface::vtoken_id(currency_id)
							.expect("Error convert vcurrency_id");
						let vtoken_amount = T::MultiCurrency::total_issuance(vcurrency_id);

						if configuration.last_block + configuration.period < n {
							let encoded_call = Self::encode_transact_call(
								configuration.contract,
								currency_id,
								token_amount,
								vtoken_amount,
							);

							let result = Self::send_xcm_to_set_token_amount(
								encoded_call,
								configuration.xcm_weight,
								configuration.xcm_fee,
							);

							if result.is_err() {
								return weight
									.saturating_add(T::DbWeight::get().reads_writes(4, 0));
							}
							Self::deposit_event(Event::XcmSetTokenAmount {
								currency_id,
								token_amount,
								vcurrency_id,
								vtoken_amount,
							});

							let mut target_fee_currency_id = GLMR;
							if T::ParachainId::get() == Id::from(BifrostKusamaChainId::get()) {
								target_fee_currency_id = MOVR;
							}

							// Will not check results and will be sent regardless of the success of
							// the burning
							let result = T::MultiCurrency::withdraw(
								target_fee_currency_id,
								&T::TreasuryAccount::get(),
								BalanceOf::<T>::unique_saturated_from(configuration.xcm_fee),
							);
							if result.is_err() {
								Self::deposit_event(Event::InsufficientAssets);
							}

							configuration.last_block = n;
							XcmEthereumCallConfiguration::<T>::put(configuration);
							currency_list.rotate_left(1);
							CurrencyIdList::<T>::put(BoundedVec::try_from(currency_list).unwrap());

							weight = weight.saturating_add(T::DbWeight::get().reads_writes(4, 2));

							is_handle_xcm_oracle = true;
						}
					},
					None => {
						weight = weight.saturating_add(T::DbWeight::get().reads_writes(2, 0));
					},
				};
			}

			if !is_handle_xcm_oracle {
				OrderQueue::<T>::mutate(|order_queue| -> DispatchResult {
					if let Some(order) = order_queue.get(0) {
						if n - order.create_block_number >= DelayBlock::<T>::get() {
							let mut order = order_queue.remove(0);
							let balance = T::MultiCurrency::free_balance(
								order.currency_id,
								&order.derivative_account,
							);
							if balance > T::MultiCurrency::minimum_balance(order.currency_id) {
								order.currency_amount = balance;
								Self::handle_order(&order)
									.map_err(|_| Error::<T>::ArgumentsError)?;
								Self::deposit_event(Event::<T>::OrderHandled {
									order: order.clone(),
								});
								weight =
									weight.saturating_add(T::DbWeight::get().reads_writes(14, 8));
							} else {
								Self::deposit_event(Event::<T>::OrderFailed { order });
								weight =
									weight.saturating_add(T::DbWeight::get().reads_writes(4, 1));
							}
						};
					};
					Ok(())
				})
				.ok();
			}
			weight
		}
	}

	#[pallet::call]
	impl<T: Config> Pallet<T> {
		/// vtoken mint and transfer to target chain
		#[pallet::call_index(0)]
		#[pallet::weight(<T as Config>::WeightInfo::mint())]
		pub fn mint(
			origin: OriginFor<T>,
			evm_caller: H160,
			currency_id: CurrencyIdOf<T>,
			target_chain: TargetChain<AccountIdOf<T>>,
			remark: BoundedVec<u8, ConstU32<32>>,
		) -> DispatchResultWithPostInfo {
			let (source_chain_caller, derivative_account, bifrost_chain_caller) =
				Self::ensure_singer_on_whitelist(origin.clone(), evm_caller, &target_chain)?;

			let order = Order {
				create_block_number: <frame_system::Pallet<T>>::block_number(),
				order_type: OrderType::Mint,
				currency_amount: Default::default(),
				source_chain_caller,
				bifrost_chain_caller,
				derivative_account,
				currency_id,
				remark,
				target_chain,
				// default to 0
				channel_id: 0u32,
			};

			OrderQueue::<T>::mutate(|order_queue| -> DispatchResultWithPostInfo {
				order_queue.try_push(order.clone()).map_err(|_| Error::<T>::ArgumentsError)?;
				Self::deposit_event(Event::<T>::CreateOrder { order });
				Ok(().into())
			})
		}

		/// Swap and transfer to target chain
		#[pallet::call_index(1)]
		#[pallet::weight(<T as Config>::WeightInfo::zenlink_swap())]
		pub fn zenlink_swap(
			origin: OriginFor<T>,
			_evm_caller: H160,
			_currency_id_in: CurrencyIdOf<T>,
			_currency_id_out: CurrencyIdOf<T>,
			_currency_id_out_min: AssetBalance,
			_target_chain: TargetChain<AccountIdOf<T>>,
		) -> DispatchResultWithPostInfo {
			ensure_signed(origin)?;
			ensure!(false, Error::<T>::Unsupported);
			Ok(().into())
		}

		/// Redeem
		#[pallet::call_index(2)]
		#[pallet::weight(<T as Config>::WeightInfo::redeem())]
		pub fn redeem(
			origin: OriginFor<T>,
			evm_caller: H160,
			vtoken_id: CurrencyIdOf<T>,
			target_chain: TargetChain<AccountIdOf<T>>,
		) -> DispatchResultWithPostInfo {
			let evm_contract_account_id = ensure_signed(origin.clone())?;
			let (source_chain_caller, derivative_account, bifrost_chain_caller) =
				Self::ensure_singer_on_whitelist(origin, evm_caller, &target_chain)?;

			if vtoken_id == VFIL {
				let fee_amount = TransferToFee::<T>::get(SupportChain::Moonbeam)
					.unwrap_or_else(|| Self::get_default_fee(BNC));
				T::MultiCurrency::transfer(
					BNC,
					&evm_contract_account_id,
					&derivative_account,
					fee_amount,
				)?;
			}

			let order = Order {
				create_block_number: <frame_system::Pallet<T>>::block_number(),
				order_type: OrderType::Redeem,
				currency_id: vtoken_id,
				currency_amount: Default::default(),
				remark: Default::default(),
				source_chain_caller,
				bifrost_chain_caller,
				derivative_account,
				target_chain,
				// default to 0
				channel_id: 0u32,
			};

			OrderQueue::<T>::mutate(|order_queue| -> DispatchResultWithPostInfo {
				order_queue.try_push(order.clone()).map_err(|_| Error::<T>::ArgumentsError)?;
				Self::deposit_event(Event::<T>::CreateOrder { order });
				Ok(().into())
			})
		}

		/// Stable pool swap
		#[pallet::call_index(3)]
		#[pallet::weight(<T as Config>::WeightInfo::stable_pool_swap())]
		pub fn stable_pool_swap(
			origin: OriginFor<T>,
			_evm_caller: H160,
			_pool_id: StableAssetPoolId,
			_currency_id_in: CurrencyIdOf<T>,
			_currency_id_out: CurrencyIdOf<T>,
			_min_dy: BalanceOf<T>,
			_target_chain: TargetChain<AccountIdOf<T>>,
		) -> DispatchResult {
			ensure_signed(origin)?;
			ensure!(false, Error::<T>::Unsupported);
			Ok(())
		}

		#[pallet::call_index(4)]
		#[pallet::weight(<T as Config>::WeightInfo::add_whitelist())]
		pub fn add_whitelist(
			origin: OriginFor<T>,
			support_chain: SupportChain,
			evm_contract_account_id: T::AccountId,
		) -> DispatchResultWithPostInfo {
			// Check the validity of origin
			T::ControlOrigin::ensure_origin(origin)?;

			let mut whitelist_account_ids = WhitelistAccountId::<T>::get(&support_chain);

			ensure!(
				!whitelist_account_ids.contains(&evm_contract_account_id),
				Error::<T>::AccountIdAlreadyInWhitelist
			);
			whitelist_account_ids
				.try_push(evm_contract_account_id.clone())
				.map_err(|_| Error::<T>::ExceededWhitelistMaxNumber)?;
			WhitelistAccountId::<T>::insert(support_chain, whitelist_account_ids);
			Self::deposit_event(Event::AddWhitelistAccountId {
				support_chain,
				evm_contract_account_id,
			});
			Ok(().into())
		}

		#[pallet::call_index(5)]
		#[pallet::weight(<T as Config>::WeightInfo::remove_whitelist())]
		pub fn remove_whitelist(
			origin: OriginFor<T>,
			support_chain: SupportChain,
			evm_contract_account_id: T::AccountId,
		) -> DispatchResultWithPostInfo {
			// Check the validity of origin
			T::ControlOrigin::ensure_origin(origin)?;

			let mut whitelist_account_ids = WhitelistAccountId::<T>::get(&support_chain);

			ensure!(
				whitelist_account_ids.contains(&evm_contract_account_id),
				Error::<T>::AccountIdNotInWhitelist
			);
			whitelist_account_ids.retain(|x| *x != evm_contract_account_id);
			WhitelistAccountId::<T>::insert(support_chain, whitelist_account_ids);
			Self::deposit_event(Event::RemoveWhitelistAccountId {
				support_chain,
				evm_contract_account_id,
			});
			Ok(().into())
		}

		#[pallet::call_index(6)]
		#[pallet::weight(<T as Config>::WeightInfo::set_execution_fee())]
		pub fn set_execution_fee(
			origin: OriginFor<T>,
			currency_id: CurrencyId,
			execution_fee: BalanceOf<T>,
		) -> DispatchResultWithPostInfo {
			// Check the validity of origin
			T::ControlOrigin::ensure_origin(origin)?;
			ExecutionFee::<T>::insert(currency_id, execution_fee);
			Self::deposit_event(Event::SetExecutionFee { currency_id, execution_fee });
			Ok(().into())
		}

		#[pallet::call_index(7)]
		#[pallet::weight(<T as Config>::WeightInfo::set_transfer_to_fee())]
		pub fn set_transfer_to_fee(
			origin: OriginFor<T>,
			support_chain: SupportChain,
			transfer_to_fee: BalanceOf<T>,
		) -> DispatchResultWithPostInfo {
			// Check the validity of origin
			T::ControlOrigin::ensure_origin(origin)?;
			TransferToFee::<T>::insert(support_chain, transfer_to_fee);
			Self::deposit_event(Event::SetTransferToFee { support_chain, transfer_to_fee });
			Ok(().into())
		}

		#[pallet::call_index(8)]
		#[pallet::weight(<T as Config>::WeightInfo::set_transfer_to_fee())]
		pub fn set_currency_ethereum_call_switch(
			origin: OriginFor<T>,
			currency_id: CurrencyId,
			is_support: bool,
		) -> DispatchResultWithPostInfo {
			// Check the validity of origin
			T::ControlOrigin::ensure_origin(origin)?;
			// Check in advance to avoid hook errors
			T::VtokenMintingInterface::vtoken_id(currency_id)
				.ok_or(Error::<T>::ErrorConvertVtoken)?;
			let mut currency_list = CurrencyIdList::<T>::get();
			match is_support {
				true => {
					ensure!(!currency_list.contains(&currency_id), Error::<T>::ArgumentsError);
					currency_list
						.try_push(currency_id)
						.map_err(|_| Error::<T>::ExceededWhitelistMaxNumber)?;
				},
				false => {
					ensure!(currency_list.contains(&currency_id), Error::<T>::ArgumentsError);
					currency_list.retain(|&x| x != currency_id);
				},
			};
			CurrencyIdList::<T>::put(currency_list);
			Self::deposit_event(Event::SetCurrencyEthereumCallSwitch { currency_id, is_support });
			Ok(().into())
		}

		#[pallet::call_index(9)]
		#[pallet::weight(<T as Config>::WeightInfo::set_transfer_to_fee())]
		pub fn set_ethereum_call_configration(
			origin: OriginFor<T>,
			xcm_fee: u128,
			xcm_weight: Weight,
			period: BlockNumberFor<T>,
			contract: H160,
		) -> DispatchResultWithPostInfo {
			T::ControlOrigin::ensure_origin(origin)?;
			XcmEthereumCallConfiguration::<T>::put(EthereumCallConfiguration {
				xcm_fee,
				xcm_weight,
				period,
				last_block: frame_system::Pallet::<T>::block_number(),
				contract,
			});
			Self::deposit_event(Event::SetEthereumCallConfiguration {
				xcm_fee,
				xcm_weight,
				period,
				contract,
			});
			Ok(().into())
		}

		#[pallet::call_index(10)]
		#[pallet::weight(T::DbWeight::get().reads(1) + T::DbWeight::get().writes(1))]
		pub fn set_currency_support_xcm_fee(
			origin: OriginFor<T>,
			currency_id: CurrencyId,
			is_support: bool,
		) -> DispatchResultWithPostInfo {
			// Check the validity of origin
			T::ControlOrigin::ensure_origin(origin)?;

			let mut currency_list = SupportXcmFeeList::<T>::get();
			match is_support {
				true => {
					ensure!(!currency_list.contains(&currency_id), Error::<T>::ArgumentsError);
					currency_list.try_push(currency_id).map_err(|_| Error::<T>::ArgumentsError)?;
				},
				false => {
					ensure!(currency_list.contains(&currency_id), Error::<T>::ArgumentsError);
					currency_list.retain(|&x| x != currency_id);
				},
			};
			SupportXcmFeeList::<T>::put(currency_list);
			Self::deposit_event(Event::SetCurrencyToSupportXcmFee { currency_id, is_support });
			Ok(().into())
		}

		#[pallet::call_index(11)]
		#[pallet::weight(T::DbWeight::get().reads(1) + T::DbWeight::get().writes(1))]
		pub fn set_delay_block(
			origin: OriginFor<T>,
			delay_block: BlockNumberFor<T>,
		) -> DispatchResultWithPostInfo {
			// Check the validity of origin
			T::ControlOrigin::ensure_origin(origin)?;
			DelayBlock::<T>::put(delay_block);
			Self::deposit_event(Event::SetDelayBlock { delay_block });
			Ok(().into())
		}

		#[pallet::call_index(12)]
		#[pallet::weight(T::DbWeight::get().reads(1) + T::DbWeight::get().writes(1))]
		pub fn force_add_order(
			origin: OriginFor<T>,
			slpx_contract_derivative_account: AccountIdOf<T>,
			evm_caller: H160,
			currency_id: CurrencyIdOf<T>,
			target_chain: TargetChain<AccountIdOf<T>>,
			remark: BoundedVec<u8, ConstU32<32>>,
			order_type: OrderType,
		) -> DispatchResultWithPostInfo {
			ensure_root(origin)?;
			let order = Order {
				create_block_number: <frame_system::Pallet<T>>::block_number(),
				currency_amount: Default::default(),
				source_chain_caller: OrderCaller::Evm(evm_caller),
				bifrost_chain_caller: slpx_contract_derivative_account,
				derivative_account: Self::h160_to_account_id(evm_caller),
				order_type,
				currency_id,
				remark,
				target_chain,
				// default to 0
				channel_id: 0u32,
			};

			OrderQueue::<T>::mutate(|order_queue| -> DispatchResultWithPostInfo {
				order_queue.try_push(order.clone()).map_err(|_| Error::<T>::ArgumentsError)?;
				Self::deposit_event(Event::<T>::CreateOrder { order });
				Ok(().into())
			})
		}

		#[pallet::call_index(13)]
		#[pallet::weight(<T as Config>::WeightInfo::mint_with_channel_id())]
		pub fn mint_with_channel_id(
			origin: OriginFor<T>,
			evm_caller: H160,
			currency_id: CurrencyIdOf<T>,
			target_chain: TargetChain<AccountIdOf<T>>,
			remark: BoundedVec<u8, ConstU32<32>>,
			channel_id: u32,
		) -> DispatchResultWithPostInfo {
			let (source_chain_caller, derivative_account, bifrost_chain_caller) =
				Self::ensure_singer_on_whitelist(origin.clone(), evm_caller, &target_chain)?;

			let order = Order {
				create_block_number: <frame_system::Pallet<T>>::block_number(),
				order_type: OrderType::Mint,
				currency_amount: Default::default(),
				source_chain_caller,
				bifrost_chain_caller,
				derivative_account,
				currency_id,
				remark,
				target_chain,
				channel_id,
			};

			OrderQueue::<T>::mutate(|order_queue| -> DispatchResultWithPostInfo {
				order_queue.try_push(order.clone()).map_err(|_| Error::<T>::ArgumentsError)?;
				Self::deposit_event(Event::<T>::CreateOrder { order });
				Ok(().into())
			})
		}
	}
}

impl<T: Config> Pallet<T> {
	fn send_xcm_to_set_token_amount(
		call: Vec<u8>,
		xcm_weight: Weight,
		xcm_fee: u128,
	) -> DispatchResult {
		let dest =
			Location::new(1, [Parachain(T::VtokenMintingInterface::get_moonbeam_parachain_id())]);

		// Moonbeam Native Token
		let asset = Asset {
			id: AssetId::from(Location::new(0, [PalletInstance(10)])),
			fun: Fungible(xcm_fee),
		};

		let xcm_message = Xcm(vec![
			WithdrawAsset(asset.clone().into()),
			BuyExecution { fees: asset, weight_limit: Unlimited },
			Transact {
				origin_kind: OriginKind::SovereignAccount,
				require_weight_at_most: xcm_weight,
				call: call.into(),
			},
			RefundSurplus,
			DepositAsset {
				assets: AllCounted(8).into(),
				beneficiary: Location::new(
					0,
					[AccountKey20 {
						network: None,
						key: Sibling::from(T::ParachainId::get()).into_account_truncating(),
					}],
				),
			},
		]);

		// Send to sovereign
		let (ticket, _price) = T::XcmSender::validate(&mut Some(dest), &mut Some(xcm_message))
			.map_err(|_| Error::<T>::ErrorValidating)?;
		T::XcmSender::deliver(ticket).map_err(|_| Error::<T>::ErrorDelivering)?;

		Ok(())
	}

	/// setTokenAmount(bytes2,uint256,uint256)
	pub fn encode_ethereum_call(
		currency_id: CurrencyId,
		token_amount: BalanceOf<T>,
		vtoken_amount: BalanceOf<T>,
	) -> Vec<u8> {
		let bytes2_currency_id: Vec<u8> = currency_id.encode()[..2].to_vec();
		let uint256_token_amount = U256::from(token_amount.saturated_into::<u128>());
		let uint256_vtoken_amount = U256::from(vtoken_amount.saturated_into::<u128>());

		let mut call = ethabi::encode(&[
			ethabi::Token::FixedBytes(bytes2_currency_id),
			ethabi::Token::Uint(uint256_token_amount),
			ethabi::Token::Uint(uint256_vtoken_amount),
		]);

		call.splice(0..0, EVM_FUNCTION_SELECTOR);
		call
	}

	pub fn encode_transact_call(
		contract: H160,
		currency_id: CurrencyId,
		token_amount: BalanceOf<T>,
		vtoken_amount: BalanceOf<T>,
	) -> Vec<u8> {
		let ethereum_call = Self::encode_ethereum_call(currency_id, token_amount, vtoken_amount);
		let transaction = EthereumXcmTransaction::V2(EthereumXcmTransactionV2 {
			gas_limit: U256::from(MAX_GAS_LIMIT),
			action: TransactionAction::Call(contract),
			value: U256::zero(),
			input: BoundedVec::try_from(ethereum_call).unwrap(),
			access_list: None,
		});
		return MoonbeamCall::EthereumXcm(EthereumXcmCall::Transact(transaction)).encode();
	}

	/// Check if the signer is in the whitelist
	fn ensure_singer_on_whitelist(
		origin: OriginFor<T>,
		evm_caller: H160,
		target_chain: &TargetChain<AccountIdOf<T>>,
	) -> Result<(OrderCaller<AccountIdOf<T>>, AccountIdOf<T>, AccountIdOf<T>), DispatchError> {
		let bifrost_chain_caller = ensure_signed(origin)?;
		let support_chain = match target_chain {
			TargetChain::Astar(_) => SupportChain::Astar,
			TargetChain::Moonbeam(_) => SupportChain::Moonbeam,
			TargetChain::Hydradx(_) => SupportChain::Hydradx,
			TargetChain::Interlay(_) => SupportChain::Interlay,
			TargetChain::Manta(_) => SupportChain::Manta,
		};

		match target_chain {
			TargetChain::Hydradx(_) | TargetChain::Manta(_) | TargetChain::Interlay(_) => Ok((
				OrderCaller::Substrate(bifrost_chain_caller.clone()),
				bifrost_chain_caller.clone(),
				bifrost_chain_caller,
			)),
			_ => {
				let whitelist_account_ids = WhitelistAccountId::<T>::get(&support_chain);
				ensure!(
					whitelist_account_ids.contains(&bifrost_chain_caller),
					Error::<T>::AccountIdNotInWhitelist
				);
				Ok((
					OrderCaller::Evm(evm_caller),
					Self::h160_to_account_id(evm_caller),
					bifrost_chain_caller,
				))
			},
		}
	}

	/// Charge an execution fee
	fn charge_execution_fee(
		currency_id: CurrencyIdOf<T>,
		evm_caller_account_id: &AccountIdOf<T>,
	) -> Result<BalanceOf<T>, DispatchError> {
		let free_balance = T::MultiCurrency::free_balance(currency_id, evm_caller_account_id);
		let execution_fee = ExecutionFee::<T>::get(currency_id)
			.unwrap_or_else(|| Self::get_default_fee(currency_id));

		T::MultiCurrency::transfer(
			currency_id,
			evm_caller_account_id,
			&T::TreasuryAccount::get(),
			execution_fee,
		)?;

		let balance_exclude_fee =
			free_balance.checked_sub(&execution_fee).ok_or(Error::<T>::FreeBalanceTooLow)?;
		Ok(balance_exclude_fee)
	}

	fn transfer_to(
		caller: AccountIdOf<T>,
		evm_contract_account_id: &AccountIdOf<T>,
		currency_id: CurrencyIdOf<T>,
		amount: BalanceOf<T>,
		target_chain: &TargetChain<AccountIdOf<T>>,
	) -> DispatchResult {
		match target_chain {
			TargetChain::Astar(receiver) => {
				let receiver = Self::h160_to_account_id(*receiver);
				let dest = Location::new(
					1,
					[
						Parachain(AstarChainId::get()),
						AccountId32 { network: None, id: receiver.encode().try_into().unwrap() },
					],
				);

				T::XcmTransfer::transfer(caller, currency_id, amount, dest, Unlimited)?;
			},
			TargetChain::Hydradx(receiver) => {
				let dest = Location::new(
					1,
					[
						Parachain(HydrationChainId::get()),
						AccountId32 { network: None, id: receiver.encode().try_into().unwrap() },
					],
				);

				T::XcmTransfer::transfer(caller, currency_id, amount, dest, Unlimited)?;
			},
			TargetChain::Interlay(receiver) => {
				let dest = Location::new(
					1,
					[
						Parachain(InterlayChainId::get()),
						AccountId32 { network: None, id: receiver.encode().try_into().unwrap() },
					],
				);

				T::XcmTransfer::transfer(caller, currency_id, amount, dest, Unlimited)?;
			},
			TargetChain::Manta(receiver) => {
				let dest = Location::new(
					1,
					[
						Parachain(MantaChainId::get()),
						AccountId32 { network: None, id: receiver.encode().try_into().unwrap() },
					],
				);

				T::XcmTransfer::transfer(caller, currency_id, amount, dest, Unlimited)?;
			},
			TargetChain::Moonbeam(receiver) => {
				let dest = Location::new(
					1,
					[
						Parachain(T::VtokenMintingInterface::get_moonbeam_parachain_id()),
						AccountKey20 { network: None, key: receiver.to_fixed_bytes() },
					],
				);
				if SupportXcmFeeList::<T>::get().contains(&currency_id) {
					T::XcmTransfer::transfer(caller, currency_id, amount, dest, Unlimited)?;
				} else {
					let fee_amount = TransferToFee::<T>::get(SupportChain::Moonbeam)
						.unwrap_or_else(|| Self::get_default_fee(BNC));
					T::MultiCurrency::transfer(BNC, evm_contract_account_id, &caller, fee_amount)?;
					let assets = vec![(currency_id, amount), (BNC, fee_amount)];

					T::XcmTransfer::transfer_multicurrencies(caller, assets, 1, dest, Unlimited)?;
				}
			},
		};
		Ok(())
	}

	fn h160_to_account_id(address: H160) -> AccountIdOf<T> {
		let mut data = [0u8; 24];
		data[0..4].copy_from_slice(b"evm:");
		data[4..24].copy_from_slice(&address[..]);
		let hash = BlakeTwo256::hash(&data);

		let account_id_32 = sp_runtime::AccountId32::from(Into::<[u8; 32]>::into(hash));
		T::AccountId::decode(&mut account_id_32.as_ref()).expect("Fail to decode address")
	}

	pub fn get_default_fee(currency_id: CurrencyId) -> BalanceOf<T> {
		let decimals = currency_id
			.decimals()
			.unwrap_or(
				T::CurrencyIdConvert::get_currency_metadata(currency_id)
					.map_or(12, |metatata| metatata.decimals.into()),
			)
			.into();

		BalanceOf::<T>::saturated_from(10u128.saturating_pow(decimals).saturating_div(100u128))
	}

	#[transactional]
	pub fn handle_order(
		order: &Order<AccountIdOf<T>, CurrencyIdOf<T>, BalanceOf<T>, BlockNumberFor<T>>,
	) -> DispatchResult {
		let currency_amount =
			Self::charge_execution_fee(order.currency_id, &order.derivative_account).unwrap();
		match order.order_type {
			OrderType::Mint => {
				T::VtokenMintingInterface::mint(
					order.derivative_account.clone(),
					order.currency_id,
					currency_amount,
					order.remark.clone(),
					Some(order.channel_id),
				)
				.map_err(|_| Error::<T>::ArgumentsError)?;
				let vtoken_id = T::VtokenMintingInterface::vtoken_id(order.currency_id)
					.ok_or(Error::<T>::ArgumentsError)?;
				let vtoken_amount =
					T::MultiCurrency::free_balance(vtoken_id, &order.derivative_account);

				Self::transfer_to(
					order.derivative_account.clone(),
					&order.bifrost_chain_caller,
					vtoken_id,
					vtoken_amount,
					&order.target_chain,
				)
				.map_err(|_| Error::<T>::ArgumentsError)?;
			},
			OrderType::Redeem => {
				let redeem_type = match order.target_chain.clone() {
					TargetChain::Astar(receiver) => {
						let receiver = Self::h160_to_account_id(receiver);
						RedeemType::Astar(receiver)
					},
					TargetChain::Moonbeam(receiver) => RedeemType::Moonbeam(receiver),
					TargetChain::Hydradx(receiver) => RedeemType::Hydradx(receiver),
					TargetChain::Interlay(receiver) => RedeemType::Interlay(receiver),
					TargetChain::Manta(receiver) => RedeemType::Manta(receiver),
				};
				T::VtokenMintingInterface::slpx_redeem(
					order.derivative_account.clone(),
					order.currency_id,
					currency_amount,
					redeem_type,
				)
				.map_err(|_| Error::<T>::ArgumentsError)?;
			},
		};
		Ok(())
	}
}

// Functions to be called by other pallets.
impl<T: Config> SlpxOperator<BalanceOf<T>> for Pallet<T> {
	fn get_moonbeam_transfer_to_fee() -> BalanceOf<T> {
		TransferToFee::<T>::get(SupportChain::Moonbeam)
			.unwrap_or_else(|| Self::get_default_fee(BNC))
	}
}
