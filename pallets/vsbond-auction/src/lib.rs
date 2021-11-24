// This file is part of Bifrost.

// Copyright (C) 2019-2021 Liebi Technologies (UK) Ltd.
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

// Ensure we're `no_std` when compiling for Wasm.
#![cfg_attr(not(feature = "std"), no_std)]

//! The pallet supports the trading functions of `vsbond`.
//!
//! Users can create sell orders by `create_order`;
//! Or buy the sell orders by `clinch_order`, `partial_clinch_order`.
//!
//! NOTE: Pallet does not support users creating buy orders by now.

use core::fmt::Debug;

use frame_support::{
	pallet_prelude::*,
	sp_runtime::{
		traits::{AtLeast32BitUnsigned, SaturatedConversion, Saturating, Zero},
		FixedPointNumber, FixedU128,
	},
};
use frame_system::pallet_prelude::*;
use node_primitives::{CurrencyId, LeasePeriod, TokenSymbol};
use orml_traits::{MultiCurrency, MultiReservableCurrency};
pub use pallet::*;
use scale_info::TypeInfo;
use sp_std::cmp::min;
pub use weights::WeightInfo;

#[cfg(test)]
mod mock;
#[cfg(test)]
mod tests;
pub mod weights;

#[cfg(feature = "runtime-benchmarks")]
mod benchmarking;

#[derive(Encode, Decode, Clone, Eq, PartialEq, Debug, TypeInfo)]
pub struct OrderInfo<AccountIdOf, BalanceOf>
where
	AccountIdOf: Debug,
	BalanceOf: Debug + AtLeast32BitUnsigned,
{
	/// The owner of the order
	owner: AccountIdOf,
	/// The vsbond type of the order to sell
	vsbond: CurrencyId,
	/// The quantity of vsbond to sell or buy
	amount: BalanceOf,
	/// The quantity of vsbond has not be sold or took
	remain: BalanceOf,
	/// Total price of the order
	total_price: BalanceOf,
	/// Helper to calculate the remain to unreserve
	remain_price: BalanceOf,
	/// The unique id of the order
	order_id: OrderId,
	order_type: OrderType,
}

impl<AccountIdOf, BalanceOf> OrderInfo<AccountIdOf, BalanceOf>
where
	AccountIdOf: Debug,
	BalanceOf: Debug + AtLeast32BitUnsigned + Copy,
{
	pub fn unit_price(&self) -> FixedU128 {
		let amount: u128 = self.amount.saturated_into();
		let total_price: u128 = self.total_price.saturated_into();

		match amount {
			0 => 0.into(),
			_ => FixedU128::from((total_price, amount)),
		}
	}
}

#[derive(Encode, Decode, Copy, Clone, Eq, PartialEq, Debug, TypeInfo)]
pub enum OrderType {
	Sell,
	Buy,
}

type OrderId = u64;
type ParaId = u32;

#[allow(type_alias_bounds)]
type AccountIdOf<T: Config> = <T as frame_system::Config>::AccountId;

#[allow(type_alias_bounds)]
type BalanceOf<T: Config<I>, I: 'static = ()> =
	<<T as Config<I>>::MultiCurrency as MultiCurrency<AccountIdOf<T>>>::Balance;

#[allow(type_alias_bounds)]
type LeasePeriodOf<T: Config> = <T as frame_system::Config>::BlockNumber;

#[frame_support::pallet]
pub mod pallet {
	use super::*;

	#[pallet::config]
	pub trait Config<I: 'static = ()>: frame_system::Config<BlockNumber = LeasePeriod> {
		type Event: From<Event<Self, I>> + IsType<<Self as frame_system::Config>::Event>;

		/// The currency type that buyer to pay
		#[pallet::constant]
		type InvoicingCurrency: Get<CurrencyId>;

		/// The amount of orders in-trade that user can hold
		#[pallet::constant]
		type MaximumOrderInTrade: Get<u32>;

		/// The sale or buy quantity needs to be greater than `MinimumSupply` to create an order
		#[pallet::constant]
		type MinimumAmount: Get<BalanceOf<Self, I>>;

		type MultiCurrency: MultiCurrency<AccountIdOf<Self>, CurrencyId = CurrencyId>
			+ MultiReservableCurrency<AccountIdOf<Self>, CurrencyId = CurrencyId>;

		/// Set default weight.
		type WeightInfo: WeightInfo;
	}

	#[pallet::error]
	pub enum Error<T, I = ()> {
		NotEnoughAmount,
		NotFindOrderInfo,
		NotEnoughBalanceToUnreserve,
		NotEnoughBalanceToReserve,
		DontHaveEnoughToPay,
		ForbidRevokeOrderNotInTrade,
		ForbidRevokeOrderWithoutOwnership,
		ForbidClinchOrderNotInTrade,
		ForbidClinchOrderWithinOwnership,
		ExceedMaximumOrderInTrade,
		InvalidVsbond,
		Unexpected,
	}

	#[pallet::event]
	#[pallet::generate_deposit(pub (crate) fn deposit_event)]
	pub enum Event<T: Config<I>, I: 'static = ()> {
		/// The order has been created.
		///
		/// [order_id, order_type, order_creator, vsbond_type, vsbond_amount, total_price]
		OrderCreated(
			OrderId,
			OrderType,
			AccountIdOf<T>,
			CurrencyId,
			BalanceOf<T, I>,
			BalanceOf<T, I>,
		),
		/// The order has been revoked.
		///
		/// [order_id, order_type, order_creator, vsbond_type, vsbond_amount, vsbond_remain,
		/// total_price]
		OrderRevoked(
			OrderId,
			OrderType,
			AccountIdOf<T>,
			CurrencyId,
			BalanceOf<T, I>,
			BalanceOf<T, I>,
			BalanceOf<T, I>,
		),
		/// The order has been clinched.
		///
		/// [order_id, order_type, order_creator, order_opponent, vsbond_type,
		/// vsbond_amount_clinched, vsbond_amount, vsbond_remain, total_price]
		OrderClinchd(
			OrderId,
			OrderType,
			AccountIdOf<T>,
			AccountIdOf<T>,
			CurrencyId,
			BalanceOf<T, I>,
			BalanceOf<T, I>,
			BalanceOf<T, I>,
			BalanceOf<T, I>,
		),
	}

	#[pallet::storage]
	#[pallet::getter(fn order_id)]
	pub(crate) type NextOrderId<T: Config<I>, I: 'static = ()> =
		StorageValue<_, OrderId, ValueQuery>;

	// Just store order ids that be in-trade.
	#[pallet::storage]
	#[pallet::getter(fn user_order_ids)]
	pub(crate) type UserOrderIds<T: Config<I>, I: 'static = ()> = StorageDoubleMap<
		_,
		Blake2_128Concat,
		AccountIdOf<T>,
		Blake2_128Concat,
		OrderType,
		BoundedVec<OrderId, T::MaximumOrderInTrade>,
		ValueQuery,
	>;

	#[pallet::storage]
	#[pallet::getter(fn order_info)]
	pub(crate) type TotalOrderInfos<T: Config<I>, I: 'static = ()> =
		StorageMap<_, Blake2_128Concat, OrderId, OrderInfo<AccountIdOf<T>, BalanceOf<T, I>>>;

	#[pallet::pallet]
	pub struct Pallet<T, I = ()>(PhantomData<(T, I)>);

	#[pallet::call]
	impl<T: Config<I>, I: 'static> Pallet<T, I> {
		/// Create a sell order or buy order to sell `vsbond`.
		#[pallet::weight(T::WeightInfo::create_order())]
		pub fn create_order(
			origin: OriginFor<T>,
			#[pallet::compact] index: ParaId,
			token_symbol: TokenSymbol,
			#[pallet::compact] first_slot: LeasePeriodOf<T>,
			#[pallet::compact] last_slot: LeasePeriodOf<T>,
			#[pallet::compact] amount: BalanceOf<T, I>,
			#[pallet::compact] total_price: BalanceOf<T, I>,
			order_type: OrderType,
		) -> DispatchResultWithPostInfo {
			// Check origin
			let owner = ensure_signed(origin)?;

			// Check amount
			ensure!(amount > T::MinimumAmount::get(), Error::<T, I>::NotEnoughAmount);

			// Check the token_symbol
			ensure!(
				token_symbol == TokenSymbol::KSM || token_symbol == TokenSymbol::DOT,
				Error::<T, I>::InvalidVsbond
			);

			// Construct vsbond
			let (_, vsbond) = CurrencyId::vsAssets(token_symbol, index, first_slot, last_slot);

			// Check the balance
			let (token_reserved, amount_reserved) = match order_type {
				OrderType::Buy => (T::InvoicingCurrency::get(), total_price),
				OrderType::Sell => (vsbond, amount),
			};

			ensure!(
				T::MultiCurrency::can_reserve(token_reserved, &owner, amount_reserved),
				Error::<T, I>::NotEnoughBalanceToReserve
			);

			let order_ids_len = Self::user_order_ids(&owner, order_type).len();
			ensure!(
				order_ids_len < T::MaximumOrderInTrade::get() as usize,
				Error::<T, I>::ExceedMaximumOrderInTrade,
			);

			// Create OrderInfo
			let order_id = Self::next_order_id();
			let order_info = OrderInfo::<AccountIdOf<T>, BalanceOf<T, I>> {
				owner: owner.clone(),
				vsbond,
				amount,
				remain: amount,
				total_price,
				remain_price: total_price,
				order_id,
				order_type,
			};

			// Reserve the balance.
			T::MultiCurrency::reserve(token_reserved, &owner, amount_reserved)?;

			// Insert OrderInfo to Storage
			TotalOrderInfos::<T, I>::insert(order_id, order_info);
			UserOrderIds::<T, I>::try_append(owner.clone(), order_type, order_id)
				.map_err(|_| Error::<T, I>::Unexpected)
				.map_err(|_| Error::<T, I>::Unexpected)?;

			Self::deposit_event(Event::OrderCreated(
				order_id,
				order_type,
				owner,
				vsbond,
				amount,
				total_price,
			));

			Ok(().into())
		}

		/// Revoke a sell or buy order in trade by the order creator.
		#[pallet::weight(T::WeightInfo::revoke_order())]
		pub fn revoke_order(
			origin: OriginFor<T>,
			#[pallet::compact] order_id: OrderId,
		) -> DispatchResultWithPostInfo {
			// Check origin
			let from = ensure_signed(origin)?;

			// Check OrderInfo
			let order_info = Self::order_info(order_id).ok_or(Error::<T, I>::NotFindOrderInfo)?;

			// Check OrderOwner
			ensure!(order_info.owner == from, Error::<T, I>::ForbidRevokeOrderWithoutOwnership);

			let (token_unreserve, amount_unreserve) = match order_info.order_type {
				OrderType::Buy => (T::InvoicingCurrency::get(), order_info.remain_price),
				OrderType::Sell => (order_info.vsbond, order_info.remain),
			};

			// To unreserve
			let reserved_balance =
				T::MultiCurrency::reserved_balance(token_unreserve, &order_info.owner);
			ensure!(
				reserved_balance >= amount_unreserve,
				Error::<T, I>::NotEnoughBalanceToUnreserve
			);
			T::MultiCurrency::unreserve(token_unreserve, &order_info.owner, amount_unreserve);

			// Revoke order
			TotalOrderInfos::<T, I>::remove(order_id);
			Self::try_to_remove_order_id(order_info.owner.clone(), order_info.order_type, order_id);

			Self::deposit_event(Event::OrderRevoked(
				order_id,
				order_info.order_type,
				order_info.owner,
				order_info.vsbond,
				order_info.amount,
				order_info.remain,
				order_info.total_price,
			));

			Ok(().into())
		}

		/// Users(non-order-creator) buy the remaining `vsbond` of a sell order.
		#[pallet::weight(T::WeightInfo::clinch_order())]
		pub fn clinch_order(
			origin: OriginFor<T>,
			#[pallet::compact] order_id: OrderId,
		) -> DispatchResultWithPostInfo {
			let order_info = Self::order_info(order_id).ok_or(Error::<T, I>::NotFindOrderInfo)?;

			Self::partial_clinch_order(origin, order_id, order_info.remain)?;

			Ok(().into())
		}

		/// Users(non-order-creator) buys some of the remaining `vsbond` of a sell or buy order.
		#[pallet::weight(T::WeightInfo::partial_clinch_order())]
		pub fn partial_clinch_order(
			origin: OriginFor<T>,
			#[pallet::compact] order_id: OrderId,
			#[pallet::compact] quantity: BalanceOf<T, I>,
		) -> DispatchResultWithPostInfo {
			// Check Zero
			if quantity.is_zero() {
				return Ok(().into());
			}

			// Check origin
			let opponent = ensure_signed(origin)?;

			// Check OrderInfo
			let order_info = Self::order_info(order_id).ok_or(Error::<T, I>::NotFindOrderInfo)?;

			// Check OrderOwner
			ensure!(order_info.owner != opponent, Error::<T, I>::ForbidClinchOrderWithinOwnership);

			// Calculate the real quantity to clinch
			let quantity_clinchd = min(order_info.remain, quantity);
			// Calculate the total price that buyer need to pay
			let price_to_pay = Self::price_to_pay(quantity_clinchd, order_info.unit_price());

			let (token_owner, amount_owner, token_opponent, amount_opponent) = match order_info
				.order_type
			{
				OrderType::Buy =>
					(T::InvoicingCurrency::get(), price_to_pay, order_info.vsbond, quantity_clinchd),
				OrderType::Sell =>
					(order_info.vsbond, quantity_clinchd, T::InvoicingCurrency::get(), price_to_pay),
			};

			// Check the balance of opponent
			T::MultiCurrency::ensure_can_withdraw(token_opponent, &opponent, amount_opponent)
				.map_err(|_| Error::<T, I>::DontHaveEnoughToPay)?;

			// Get the new OrderInfo
			let new_order_info = if quantity_clinchd == order_info.remain {
				OrderInfo {
					remain: Zero::zero(),
					remain_price: order_info.remain_price.saturating_sub(price_to_pay),
					..order_info
				}
			} else {
				OrderInfo {
					remain: order_info.remain.saturating_sub(quantity_clinchd),
					remain_price: order_info.remain_price.saturating_sub(price_to_pay),
					..order_info
				}
			};

			// Unreserve the balance
			let reserved_balance =
				T::MultiCurrency::reserved_balance(token_owner, &new_order_info.owner);
			ensure!(reserved_balance >= amount_owner, Error::<T, I>::NotEnoughBalanceToUnreserve);
			T::MultiCurrency::unreserve(token_owner, &new_order_info.owner, amount_owner);

			// Exchange: Transfer assets to opponent
			T::MultiCurrency::transfer(
				token_owner,
				&new_order_info.owner,
				&opponent,
				amount_owner,
			)?;
			// Exchange: Transfer assets to owner
			T::MultiCurrency::transfer(
				token_opponent,
				&opponent,
				&new_order_info.owner,
				amount_opponent,
			)?;

			// Change the OrderInfo in Storage
			if new_order_info.remain == Zero::zero() {
				TotalOrderInfos::<T, I>::remove(order_id);
				Self::try_to_remove_order_id(
					new_order_info.owner.clone(),
					order_info.order_type,
					order_id,
				);

				if new_order_info.order_type == OrderType::Buy {
					T::MultiCurrency::unreserve(
						token_owner,
						&new_order_info.owner,
						new_order_info.remain_price,
					);
				}
			} else {
				TotalOrderInfos::<T, I>::insert(order_id, new_order_info.clone());
			}

			Self::deposit_event(Event::<T, I>::OrderClinchd(
				order_id,
				new_order_info.order_type,
				new_order_info.owner,
				opponent,
				new_order_info.vsbond,
				quantity_clinchd,
				new_order_info.amount,
				new_order_info.remain,
				new_order_info.total_price,
			));

			Ok(().into())
		}
	}

	impl<T: Config<I>, I: 'static> Pallet<T, I> {
		pub(crate) fn next_order_id() -> OrderId {
			let next_order_id = Self::order_id();
			NextOrderId::<T, I>::mutate(|current| *current += 1);
			next_order_id
		}

		pub(crate) fn try_to_remove_order_id(
			account: AccountIdOf<T>,
			order_type: OrderType,
			order_id: OrderId,
		) {
			UserOrderIds::<T, I>::mutate(account, order_type, |order_ids| {
				if let Some(position) = order_ids.iter().position(|&r| r == order_id) {
					order_ids.remove(position);
				}
			});
		}

		/// Get the price(round up) needed to pay.
		pub(crate) fn price_to_pay(
			quantity: BalanceOf<T, I>,
			unit_price: FixedU128,
		) -> BalanceOf<T, I> {
			let quantity: u128 = quantity.saturated_into();

			let total_price = (unit_price.saturating_mul(quantity.into())).floor().into_inner() /
				FixedU128::accuracy();

			BalanceOf::<T, I>::saturated_from(total_price)
		}
	}
}

// TODO: Maybe impl Auction trait for vsbond-auction
