pub mod helpers;

use candid::{Nat, Principal};
use pocket_ic::PocketIc;

use crate::{
    candid_types::CandidBlockTag,
    lifecycle::{InitArg, MinterArg},
    tests::pocket_ic_helpers::{sender_principal, MINTER_WASM_BYTES},
};

#[test]
fn name() {
    todo!();
}
