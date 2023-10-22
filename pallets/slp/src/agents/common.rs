// This file is part of Bifrost.

// Copyright (C) 2019-2022 Liebi Technologies (UK) Ltd.
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
use crate::{
	pallet::Error,
	vec, AccountIdOf, BalanceOf, BoundedVec, Box, Config, CurrencyLatestTuneRecord,
	DelegatorLatestTuneRecord, DelegatorLedgers, DelegatorNextIndex, DelegatorsIndex2Multilocation,
	DelegatorsMultilocation2Index, Encode, Event,
	Junction::{AccountId32, Parachain},
	Junctions::{Here, X1},
	MinimumsAndMaximums, MultiLocation, Pallet, Validators, Xcm, XcmOperationType, Zero,
};
use frame_support::{ensure, traits::Len};
use node_primitives::{
	traits::BridgeOperator, CurrencyId, VtokenMintingOperator, XcmDestWeightAndFeeHandler,
};
use orml_traits::MultiCurrency;
use sp_core::{Get, U256};
use sp_runtime::{
	traits::{UniqueSaturatedFrom, UniqueSaturatedInto},
	DispatchResult,
};
use xcm::{v3::prelude::*, VersionedMultiLocation};

// Some common business functions for all agents
impl<T: Config> Pallet<T> {
	pub(crate) fn inner_initialize_delegator(currency_id: CurrencyId) -> Result<u16, Error<T>> {
		let new_delegator_id = DelegatorNextIndex::<T>::get(currency_id);
		DelegatorNextIndex::<T>::mutate(currency_id, |id| -> Result<(), Error<T>> {
			let option_new_id = id.checked_add(1).ok_or(Error::<T>::OverFlow)?;
			*id = option_new_id;
			Ok(())
		})?;

		Ok(new_delegator_id)
	}

	/// Add a new serving delegator for a particular currency.
	pub(crate) fn inner_add_delegator(
		index: u16,
		who: &MultiLocation,
		currency_id: CurrencyId,
	) -> DispatchResult {
		// Check if the delegator already exists. If yes, return error.
		ensure!(
			!DelegatorsIndex2Multilocation::<T>::contains_key(currency_id, index),
			Error::<T>::AlreadyExist
		);

		// Ensure delegators count is not greater than maximum.
		let delegators_count = DelegatorNextIndex::<T>::get(currency_id);
		let mins_maxs = MinimumsAndMaximums::<T>::get(currency_id).ok_or(Error::<T>::NotExist)?;
		ensure!(delegators_count < mins_maxs.delegators_maximum, Error::<T>::GreaterThanMaximum);

		// Revise two delegator storages.
		DelegatorsIndex2Multilocation::<T>::insert(currency_id, index, who);
		DelegatorsMultilocation2Index::<T>::insert(currency_id, who, index);

		Ok(())
	}

	pub(crate) fn inner_add_validator(
		who: &MultiLocation,
		currency_id: CurrencyId,
	) -> DispatchResult {
		// Check if the validator already exists.
		let validators_set = Validators::<T>::get(currency_id);

		// Ensure validator candidates in the whitelist is not greater than maximum.
		let mins_maxs = MinimumsAndMaximums::<T>::get(currency_id).ok_or(Error::<T>::NotExist)?;
		ensure!(
			validators_set.len() as u16 <= mins_maxs.validators_maximum,
			Error::<T>::GreaterThanMaximum
		);

		// ensure validator candidates are less than MaxLengthLimit
		ensure!(
			validators_set.len() < T::MaxLengthLimit::get() as usize,
			Error::<T>::ExceedMaxLengthLimit
		);

		let mut validators_vec;
		if let Some(validators_bounded_vec) = validators_set {
			validators_vec = validators_bounded_vec.to_vec();
			let rs = validators_vec.iter().position(|multi| multi == who);
			// Check if the validator is in the already exist.
			ensure!(rs.is_none(), Error::<T>::AlreadyExist);

			// If the validator is not in the whitelist, add it.
			validators_vec.push(*who);
		} else {
			validators_vec = vec![*who];
		}

		let bounded_list =
			BoundedVec::try_from(validators_vec).map_err(|_| Error::<T>::FailToConvert)?;

		Validators::<T>::insert(currency_id, bounded_list);

		// Deposit event.
		Pallet::<T>::deposit_event(Event::ValidatorsAdded { currency_id, validator_id: *who });

		Ok(())
	}

	pub(crate) fn inner_remove_delegator(
		who: &MultiLocation,
		currency_id: CurrencyId,
	) -> DispatchResult {
		// Check if the delegator exists.
		let index = DelegatorsMultilocation2Index::<T>::get(currency_id, who)
			.ok_or(Error::<T>::DelegatorNotExist)?;
		// Remove corresponding storage.
		DelegatorsIndex2Multilocation::<T>::remove(currency_id, index);
		DelegatorsMultilocation2Index::<T>::remove(currency_id, who);
		DelegatorLedgers::<T>::remove(currency_id, who);

		Ok(())
	}

	/// Remove an existing serving delegator for a particular currency.
	pub(crate) fn inner_remove_validator(
		who: &MultiLocation,
		currency_id: CurrencyId,
	) -> DispatchResult {
		// Check if the validator already exists.
		let validators_set =
			Validators::<T>::get(currency_id).ok_or(Error::<T>::ValidatorSetNotExist)?;

		ensure!(validators_set.contains(who), Error::<T>::ValidatorNotExist);

		// Update corresponding storage.
		Validators::<T>::mutate(currency_id, |validator_vec| {
			if let Some(ref mut validator_list) = validator_vec {
				let index_op = validator_list.clone().iter().position(|va| va == who);

				if let Some(index) = index_op {
					validator_list.remove(index);

					Pallet::<T>::deposit_event(Event::ValidatorsRemoved {
						currency_id,
						validator_id: *who,
					});
				}
			}
		});

		Ok(())
	}

	/// Charge vtoken for hosting fee.
	pub(crate) fn inner_calculate_vtoken_hosting_fee(
		amount: BalanceOf<T>,
		vtoken: CurrencyId,
		currency_id: CurrencyId,
	) -> Result<BalanceOf<T>, Error<T>> {
		ensure!(amount > Zero::zero(), Error::<T>::AmountZero);

		let vtoken_issuance = T::MultiCurrency::total_issuance(vtoken);
		let token_pool = T::VtokenMinting::get_token_pool(currency_id);
		// Calculate how much vksm the beneficiary account can get.
		let amount: u128 = amount.unique_saturated_into();
		let vtoken_issuance: u128 = vtoken_issuance.unique_saturated_into();
		let token_pool: u128 = token_pool.unique_saturated_into();
		let can_get_vtoken = U256::from(amount)
			.checked_mul(U256::from(vtoken_issuance))
			.and_then(|n| n.checked_div(U256::from(token_pool)))
			.and_then(|n| TryInto::<u128>::try_into(n).ok())
			.unwrap_or_else(Zero::zero);

		let charge_amount = BalanceOf::<T>::unique_saturated_from(can_get_vtoken);

		Ok(charge_amount)
	}

	pub(crate) fn inner_charge_hosting_fee(
		charge_amount: BalanceOf<T>,
		to: &MultiLocation,
		depoist_currency: CurrencyId,
	) -> DispatchResult {
		ensure!(charge_amount > Zero::zero(), Error::<T>::AmountZero);

		let beneficiary = Pallet::<T>::multilocation_to_account(&to)?;
		// Issue corresponding vksm to beneficiary account.
		T::MultiCurrency::deposit(depoist_currency, &beneficiary, charge_amount)?;

		Ok(())
	}

	pub(crate) fn get_transfer_back_dest_and_beneficiary(
		from: &MultiLocation,
		to: &MultiLocation,
		currency_id: CurrencyId,
	) -> Result<(Box<VersionedMultiLocation>, Box<VersionedMultiLocation>), Error<T>> {
		// Check if from is one of our delegators. If not, return error.
		DelegatorsMultilocation2Index::<T>::get(currency_id, from)
			.ok_or(Error::<T>::DelegatorNotExist)?;

		// Make sure the receiving account is the Exit_account from vtoken-minting module.
		let to_account_id = Pallet::<T>::multilocation_to_account(to)?;
		let (_, exit_account) = T::VtokenMinting::get_entrance_and_exit_accounts();
		ensure!(to_account_id == exit_account, Error::<T>::InvalidAccount);

		// Prepare parameter dest and beneficiary.
		let to_32: [u8; 32] = Pallet::<T>::multilocation_to_account_32(to)?;

		let dest = Box::new(VersionedMultiLocation::from(MultiLocation::from(X1(Parachain(
			T::ParachainId::get().into(),
		)))));

		let beneficiary =
			Box::new(VersionedMultiLocation::from(MultiLocation::from(X1(AccountId32 {
				network: None,
				id: to_32,
			}))));

		Ok((dest, beneficiary))
	}

	pub(crate) fn inner_do_transfer_to(
		from: &MultiLocation,
		to: &MultiLocation,
		amount: BalanceOf<T>,
		currency_id: CurrencyId,
		assets: MultiAssets,
		dest: &MultiLocation,
	) -> Result<(), Error<T>> {
		// Ensure amount is greater than zero.
		ensure!(!amount.is_zero(), Error::<T>::AmountZero);

		// Ensure the from account is located within Bifrost chain. Otherwise, the xcm massage will
		// not succeed.
		ensure!(from.parents.is_zero(), Error::<T>::InvalidTransferSource);

		let (weight, fee_amount) = T::XcmWeightAndFeeHandler::get_operation_weight_and_fee(
			currency_id,
			XcmOperationType::TransferTo,
		)
		.ok_or(Error::<T>::WeightAndFeeNotExists)?;

		// Prepare parameter beneficiary.
		let to_32: [u8; 32] = Pallet::<T>::multilocation_to_account_32(to)?;
		let beneficiary = Pallet::<T>::account_32_to_local_location(to_32)?;

		// Prepare fee asset.
		let fee_asset = MultiAsset {
			fun: Fungible(fee_amount.unique_saturated_into()),
			id: Concrete(MultiLocation { parents: 0, interior: Here }),
		};

		// prepare for xcm message
		let msg = Xcm(vec![
			WithdrawAsset(assets),
			InitiateReserveWithdraw {
				assets: All.into(),
				reserve: *dest,
				xcm: Xcm(vec![
					BuyExecution { fees: fee_asset, weight_limit: WeightLimit::Limited(weight) },
					DepositAsset { assets: AllCounted(1).into(), beneficiary },
				]),
			},
		]);
		let hash = msg.using_encoded(sp_io::hashing::blake2_256);
		// Execute the xcm message.
		T::XcmExecutor::execute_xcm_in_credit(*from, msg, hash, weight, weight)
			.ensure_complete()
			.map_err(|_| Error::<T>::XcmFailure)?;

		Ok(())
	}

	pub(crate) fn tune_vtoken_exchange_rate_without_update_ledger(
		who: &MultiLocation,
		token_amount: BalanceOf<T>,
		currency_id: CurrencyId,
	) -> Result<(), Error<T>> {
		// ensure who is a valid delegator
		ensure!(
			DelegatorsMultilocation2Index::<T>::contains_key(currency_id, &who),
			Error::<T>::DelegatorNotExist
		);

		// Get current TimeUnit.
		let current_time_unit = T::VtokenMinting::get_ongoing_time_unit(currency_id)
			.ok_or(Error::<T>::TimeUnitNotExist)?;
		// Get DelegatorLatestTuneRecord for the currencyId.
		let latest_time_unit_op = DelegatorLatestTuneRecord::<T>::get(currency_id, &who);
		// ensure each delegator can only tune once per TimeUnit.
		ensure!(
			latest_time_unit_op != Some(current_time_unit.clone()),
			Error::<T>::DelegatorAlreadyTuned
		);

		ensure!(!token_amount.is_zero(), Error::<T>::AmountZero);

		// Check whether "who" is an existing delegator.
		ensure!(
			DelegatorLedgers::<T>::contains_key(currency_id, who),
			Error::<T>::DelegatorNotBonded
		);

		// Tune the vtoken exchange rate.
		T::VtokenMinting::increase_token_pool(currency_id, token_amount)
			.map_err(|_| Error::<T>::IncreaseTokenPoolError)?;

		// Update the DelegatorLatestTuneRecord<T> storage.
		DelegatorLatestTuneRecord::<T>::insert(currency_id, who, current_time_unit);

		Ok(())
	}

	pub(crate) fn send_message(
		operation: XcmOperationType,
		fee_payer: AccountIdOf<T>,
		to_location: &MultiLocation,
		amount: BalanceOf<T>,
		currency_id: CurrencyId,
		dest_native_currency_id: CurrencyId,
	) -> Result<(), Error<T>> {
		let (_network_id, dst_chain) =
			T::BridgeOperator::get_chain_network_and_id(dest_native_currency_id)
				.map_err(|_| Error::<T>::NetworkIdError)?;

		let receiver = T::BridgeOperator::get_receiver_from_multilocation(
			dest_native_currency_id,
			to_location,
		)
		.map_err(|_| Error::<T>::FailToConvert)?;
		let payload = T::BridgeOperator::get_cross_out_payload(
			operation,
			currency_id,
			amount,
			Some(&receiver),
		)
		.map_err(|_| Error::<T>::FailToGetPayload)?;

		T::BridgeOperator::send_message_to_anchor(fee_payer, dst_chain, &payload)
			.map_err(|_| Error::<T>::FailToSendCrossOutMessage)?;

		Ok(())
	}

	pub(crate) fn check_tuning_limit(currency_id: CurrencyId) -> Result<u32, Error<T>> {
		// Get current TimeUnit.
		let current_time_unit = T::VtokenMinting::get_ongoing_time_unit(currency_id)
			.ok_or(Error::<T>::TimeUnitNotExist)?;
		// If this is the first time.
		if !CurrencyLatestTuneRecord::<T>::contains_key(currency_id) {
			// Insert an empty record into CurrencyLatestTuneRecord storage.
			CurrencyLatestTuneRecord::<T>::insert(currency_id, (current_time_unit.clone(), 0));
		}

		// Get CurrencyLatestTuneRecord for the currencyId.
		let (latest_time_unit, tune_num) = Self::get_currency_latest_tune_record(currency_id)
			.ok_or(Error::<T>::CurrencyLatestTuneRecordNotExist)?;

		// See if exceeds tuning limit.
		// If it has been tuned in the current time unit, ensure this tuning is within limit.
		let (limit_num, _) = Self::get_currency_tune_exchange_rate_limit(currency_id)
			.ok_or(Error::<T>::TuneExchangeRateLimitNotSet)?;
		let mut new_tune_num = Zero::zero();
		if latest_time_unit == current_time_unit {
			ensure!(tune_num < limit_num, Error::<T>::GreaterThanMaximum);
			new_tune_num = tune_num;
		}

		new_tune_num = new_tune_num.checked_add(1).ok_or(Error::<T>::OverFlow)?;

		Ok(new_tune_num)
	}
}
