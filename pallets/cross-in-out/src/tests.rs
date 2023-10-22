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

#![cfg(test)]

use crate::{mock::*, *};
use frame_support::{assert_noop, assert_ok};
use node_primitives::currency::KSM;
use sp_runtime::DispatchError::BadOrigin;

const FILECOIN_NETWORK_ID: xcm::v3::NetworkId = NetworkId::Ethereum { chain_id: 314159 };

#[test]
fn cross_in_and_cross_out_should_work() {
	ExtBuilder::default().one_hundred_for_alice_n_bob().build().execute_with(|| {
		let location = MultiLocation {
			parents: 100,
			interior: X1(Junction::AccountKey20 {
				network: Some(FILECOIN_NETWORK_ID),
				key: [0u8; 20],
			}),
		};

		assert_noop!(
			CrossInOut::cross_in(
				RuntimeOrigin::signed(ALICE),
				Box::new(location.clone()),
				KSM,
				100,
				None,
				KSM
			),
			Error::<Runtime>::NotAllowed
		);

		let bounded_vector = BoundedVec::try_from(vec![ALICE]).unwrap();
		IssueWhiteList::<Runtime>::insert(KSM, bounded_vector);

		assert_noop!(
			CrossInOut::cross_in(
				RuntimeOrigin::signed(ALICE),
				Box::new(location.clone()),
				KSM,
				100,
				None,
				KSM
			),
			Error::<Runtime>::CurrencyNotSupportCrossInAndOut
		);

		CrossCurrencyRegistry::<Runtime>::insert(KSM, ());

		assert_noop!(
			CrossInOut::cross_in(
				RuntimeOrigin::signed(ALICE),
				Box::new(location.clone()),
				KSM,
				100,
				None,
				KSM
			),
			Error::<Runtime>::NoCrossingMinimumSet
		);

		CrossingMinimumAmount::<Runtime>::insert(KSM, (1000, 1000));

		assert_noop!(
			CrossInOut::cross_in(
				RuntimeOrigin::signed(ALICE),
				Box::new(location.clone()),
				KSM,
				100,
				None,
				KSM
			),
			Error::<Runtime>::AmountLowerThanMinimum
		);

		CrossingMinimumAmount::<Runtime>::insert(KSM, (1, 1));

		assert_noop!(
			CrossInOut::cross_in(
				RuntimeOrigin::signed(ALICE),
				Box::new(location.clone()),
				KSM,
				100,
				None,
				KSM
			),
			Error::<Runtime>::NoAccountIdMapping
		);

		AccountToOuterMultilocation::<Runtime>::insert(KSM, ALICE, location.clone());
		OuterMultilocationToAccount::<Runtime>::insert(KSM, location.clone(), ALICE);

		assert_eq!(Tokens::free_balance(KSM, &ALICE), 0);
		assert_ok!(CrossInOut::cross_in(
			RuntimeOrigin::signed(ALICE),
			Box::new(location),
			KSM,
			100,
			None,
			KSM
		));
		assert_eq!(Tokens::free_balance(KSM, &ALICE), 100);

		assert_ok!(CrossInOut::cross_out(RuntimeOrigin::signed(ALICE), KSM, 50, KSM));
		assert_eq!(Tokens::free_balance(KSM, &ALICE), 50);
	});
}

#[test]
fn add_to_and_remove_from_issue_whitelist_should_work() {
	ExtBuilder::default().one_hundred_for_alice_n_bob().build().execute_with(|| {
		assert_eq!(CrossInOut::get_issue_whitelist(KSM), None);

		assert_ok!(CrossInOut::add_to_issue_whitelist(RuntimeOrigin::signed(ALICE), KSM, ALICE));
		let bounded_vector = BoundedVec::try_from(vec![ALICE]).unwrap();
		assert_eq!(CrossInOut::get_issue_whitelist(KSM), Some(bounded_vector));

		assert_noop!(
			CrossInOut::remove_from_issue_whitelist(RuntimeOrigin::signed(ALICE), KSM, BOB),
			Error::<Runtime>::NotExist
		);

		assert_ok!(CrossInOut::remove_from_issue_whitelist(
			RuntimeOrigin::signed(ALICE),
			KSM,
			ALICE
		));
		let empty_vec = BoundedVec::default();
		assert_eq!(CrossInOut::get_issue_whitelist(KSM), Some(empty_vec));
	});
}

#[test]
fn add_to_and_remove_from_register_whitelist_should_work() {
	ExtBuilder::default().one_hundred_for_alice_n_bob().build().execute_with(|| {
		assert_eq!(CrossInOut::get_register_whitelist(KSM), None);

		assert_ok!(CrossInOut::add_to_register_whitelist(RuntimeOrigin::signed(ALICE), KSM, ALICE));
		assert_eq!(CrossInOut::get_register_whitelist(KSM), Some(vec![ALICE]));

		assert_noop!(
			CrossInOut::remove_from_register_whitelist(RuntimeOrigin::signed(ALICE), KSM, BOB),
			Error::<Runtime>::NotExist
		);

		assert_ok!(CrossInOut::remove_from_register_whitelist(
			RuntimeOrigin::signed(ALICE),
			KSM,
			ALICE
		));
		assert_eq!(CrossInOut::get_register_whitelist(KSM), Some(vec![]));
	});
}

#[test]
fn register_linked_account_should_work() {
	ExtBuilder::default().one_hundred_for_alice_n_bob().build().execute_with(|| {
		let location = MultiLocation {
			parents: 100,
			interior: X1(Junction::AccountKey20 {
				network: Some(FILECOIN_NETWORK_ID),
				key: [0u8; 20],
			}),
		};

		let location2 = MultiLocation {
			parents: 111,
			interior: X1(Junction::AccountKey20 {
				network: Some(FILECOIN_NETWORK_ID),
				key: [0u8; 20],
			}),
		};

		assert_noop!(
			CrossInOut::register_linked_account(
				RuntimeOrigin::signed(ALICE),
				KSM,
				BOB,
				Box::new(location.clone())
			),
			Error::<Runtime>::NotAllowed
		);

		RegisterWhiteList::<Runtime>::insert(KSM, vec![ALICE]);

		assert_noop!(
			CrossInOut::register_linked_account(
				RuntimeOrigin::signed(ALICE),
				KSM,
				BOB,
				Box::new(location.clone())
			),
			Error::<Runtime>::CurrencyNotSupportCrossInAndOut
		);

		CrossCurrencyRegistry::<Runtime>::insert(KSM, ());

		assert_ok!(CrossInOut::register_linked_account(
			RuntimeOrigin::signed(ALICE),
			KSM,
			ALICE,
			Box::new(location.clone())
		));

		assert_noop!(
			CrossInOut::register_linked_account(
				RuntimeOrigin::signed(ALICE),
				KSM,
				ALICE,
				Box::new(location2)
			),
			Error::<Runtime>::AlreadyExist
		);
	});
}

#[test]
fn register_and_deregister_currency_for_cross_in_out_should_work() {
	ExtBuilder::default().one_hundred_for_alice_n_bob().build().execute_with(|| {
		assert_ok!(CrossInOut::register_currency_for_cross_in_out(
			RuntimeOrigin::signed(ALICE),
			KSM,
		));

		assert_eq!(CrossCurrencyRegistry::<Runtime>::get(KSM), Some(()));

		assert_ok!(CrossInOut::deregister_currency_for_cross_in_out(
			RuntimeOrigin::signed(ALICE),
			KSM,
		));

		assert_eq!(CrossCurrencyRegistry::<Runtime>::get(KSM), None);
	});
}

#[test]
fn change_outer_linked_account_should_work() {
	ExtBuilder::default().one_hundred_for_alice_n_bob().build().execute_with(|| {
		let location = MultiLocation {
			parents: 100,
			interior: X1(Junction::AccountKey20 {
				network: Some(FILECOIN_NETWORK_ID),
				key: [0u8; 20],
			}),
		};

		let location2 = MultiLocation {
			parents: 111,
			interior: X1(Junction::AccountKey20 {
				network: Some(FILECOIN_NETWORK_ID),
				key: [0u8; 20],
			}),
		};

		AccountToOuterMultilocation::<Runtime>::insert(KSM, BOB, location.clone());
		OuterMultilocationToAccount::<Runtime>::insert(KSM, location.clone(), BOB);

		assert_noop!(
			CrossInOut::change_outer_linked_account(
				RuntimeOrigin::signed(BOB),
				KSM,
				Box::new(location2.clone()),
				BOB
			),
			BadOrigin
		);

		assert_noop!(
			CrossInOut::change_outer_linked_account(
				RuntimeOrigin::signed(ALICE),
				KSM,
				Box::new(location.clone()),
				BOB
			),
			Error::<Runtime>::CurrencyNotSupportCrossInAndOut
		);

		CrossCurrencyRegistry::<Runtime>::insert(KSM, ());

		assert_noop!(
			CrossInOut::change_outer_linked_account(
				RuntimeOrigin::signed(ALICE),
				KSM,
				Box::new(location.clone()),
				BOB
			),
			Error::<Runtime>::AlreadyExist
		);

		assert_ok!(CrossInOut::change_outer_linked_account(
			RuntimeOrigin::signed(ALICE),
			KSM,
			Box::new(location2.clone()),
			BOB
		));
	});
}

#[test]
fn set_crossing_minimum_amount_should_work() {
	ExtBuilder::default().one_hundred_for_alice_n_bob().build().execute_with(|| {
		assert_noop!(
			CrossInOut::set_crossing_minimum_amount(
				RuntimeOrigin::signed(BOB),
				KSM,
				Some((100, 100))
			),
			BadOrigin
		);

		assert_ok!(CrossInOut::set_crossing_minimum_amount(
			RuntimeOrigin::signed(ALICE),
			KSM,
			Some((100, 100))
		));

		assert_eq!(CrossingMinimumAmount::<Runtime>::get(KSM), Some((100, 100)));
	});
}
