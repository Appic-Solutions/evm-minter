// update chain data args, the off chain service calls an update endpoint to update the latest
// blocknumber and the fee hisotry, so there is no need foron chain rpc calls.
// latest block number and fee history can not introduce serious security problems so it is fine if
// we update them via an off chain service on an interval basis.

use candid::Nat;
use candid::{CandidType, Deserialize};

#[derive(CandidType, Deserialize, Clone, Debug, PartialEq)]
pub struct ChainData {
    pub latest_block_number: Nat,
    pub fee_history: String,
}
