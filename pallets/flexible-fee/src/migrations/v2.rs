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

use crate::*;
use frame_support::traits::OnRuntimeUpgrade;

const LOG_TARGET: &str = "flexible-fee::migration";

pub struct FlexibleFeeMigration<T>(sp_std::marker::PhantomData<T>);
impl<T: Config> OnRuntimeUpgrade for FlexibleFeeMigration<T> {
	fn on_runtime_upgrade() -> frame_support::weights::Weight {
		// Check the storage version
		let onchain_version = Pallet::<T>::on_chain_storage_version();
		if onchain_version < 2 {
			log::info!(target: LOG_TARGET, "Start to migrate flexible-fee storage...");
			// Remove the UserFeeChargeOrderList storage content
			let count = UserFeeChargeOrderList::<T>::clear(u32::MAX, None).unique as u64;

			// Update the storage version
			StorageVersion::new(2).put::<Pallet<T>>();

			// Return the consumed weight
			Weight::from(T::DbWeight::get().reads_writes(count as u64 + 1, count as u64 + 1))
		} else {
			// We don't do anything here.
			Weight::zero()
		}
	}

	#[cfg(feature = "try-runtime")]
	fn pre_upgrade() -> Result<Vec<u8>, &'static str> {
		let cnt = UserFeeChargeOrderList::<T>::iter().count();

		// print out the pre-migrate storage count
		log::info!(
			target: LOG_TARGET,
			"UserFeeChargeOrderList pre-migrate storage count: {:?}",
			cnt
		);
		Ok((cnt as u64).encode())
	}

	#[cfg(feature = "try-runtime")]
	fn post_upgrade(cnt: Vec<u8>) -> Result<(), &'static str> {
		let new_count = UserFeeChargeOrderList::<T>::iter().count();
		// decode cnt to u64
		let old_count = u64::decode(&mut &cnt[..]).unwrap();

		// print out the post-migrate storage count
		log::info!(
			target: LOG_TARGET,
			"UserFeeChargeOrderList post-migrate storage count: {:?}",
			UserFeeChargeOrderList::<T>::iter().count()
		);

		ensure!(
			new_count as u64 == old_count,
			"Post-migration storage count does not match pre-migration count"
		);
		Ok(())
	}
}
