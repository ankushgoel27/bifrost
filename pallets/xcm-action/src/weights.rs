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
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

//! Autogenerated weights for bifrost_xcm_action
//!
//! THIS FILE WAS AUTO-GENERATED USING THE SUBSTRATE BENCHMARK CLI VERSION 4.0.0-dev
//! DATE: 2022-07-31, STEPS: `50`, REPEAT: 20, LOW RANGE: `[]`, HIGH RANGE: `[]`
//! HOSTNAME: `localhost`, CPU: `<UNKNOWN>`
//! EXECUTION: Some(Wasm), WASM-EXECUTION: Compiled, CHAIN: Some("bifrost-kusama-local"), DB CACHE: 1024

// Executed Command:
// target/release/bifrost
// benchmark
// pallet
// --chain=bifrost-kusama-local
// --steps=50
// --repeat=20
// --pallet=bifrost_xcm_action
// --extrinsic=*
// --execution=wasm
// --wasm-execution=compiled
// --heap-pages=4096
// --output=./runtime/bifrost-kusama/src/weights2/bifrost_xcm_action.rs
// --template=./frame-weight-template.hbs

#![cfg_attr(rustfmt, rustfmt_skip)]
#![allow(unused_parens)]
#![allow(unused_imports)]

use frame_support::{traits::Get, weights::{Weight, constants::RocksDbWeight}};
use sp_std::marker::PhantomData;

/// Weight functions needed for bifrost_xcm_action.
pub trait WeightInfo {
  fn mint() -> Weight;
  fn redeem() -> Weight;
  fn swap() -> Weight;
	fn claim() -> Weight;
	fn check_origin() -> Weight;
}

/// Weights for bifrost_system_staking using the Bifrost node and recommended hardware.
pub struct BifrostWeight<T>(PhantomData<T>);
impl<T: frame_system::Config> WeightInfo for BifrostWeight<T> {
	fn mint() -> Weight {
		(18_000_000 as Weight)
	}
	fn redeem() -> Weight {
		(19_000_000 as Weight)
	}
	fn swap() -> Weight {
		(24_000_000 as Weight)
	}
	fn claim() -> Weight {
		(19_000_000 as Weight)
	}

	fn check_origin() -> Weight {
		(19_000_000 as Weight)
	}
}

// For backwards compatibility and tests
impl WeightInfo for () {
	fn mint() -> Weight {
		(18_000_000 as Weight)
	}
	fn redeem() -> Weight {
		(19_000_000 as Weight)
	}
	fn swap() -> Weight {
		(24_000_000 as Weight)
	}
	fn claim() -> Weight {
		(19_000_000 as Weight)
	}

	fn check_origin() -> Weight {
		(19_000_000 as Weight)
	}
}
