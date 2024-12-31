use candid::{CandidType, Deserialize, Nat, Principal};

#[derive(CandidType, Deserialize, Debug)]
pub struct InitArgs {
    pub minters: Vec<MinterArgs>,
}

pub type CandidChainId = Nat;

#[derive(CandidType, Deserialize, Debug)]
pub struct UpgradeArg {
    pub new_minters: Option<Vec<MinterArgs>>,
    pub update_minters: Option<Vec<UpdateMinterArgs>>,
}

#[derive(CandidType, Deserialize, Debug)]
pub enum LoggerArgs {
    Init(InitArgs),
    Upgrade(UpgradeArg),
}

#[derive(CandidType, Debug, Deserialize)]
pub enum Operator {
    DfinityCkEthMinter,
    AppicMinter,
}

#[derive(CandidType, Deserialize, Debug)]
pub struct MinterArgs {
    pub chain_id: CandidChainId,
    pub minter_id: Principal,
    pub operator: Operator,
    pub last_observed_event: Nat,
    pub last_scraped_event: Nat,
    pub evm_to_icp_fee: Nat,
    pub icp_to_evm_fee: Nat,
}

#[derive(CandidType, Deserialize, Debug)]
pub struct UpdateMinterArgs {
    pub chain_id: CandidChainId,
    pub minter_id: Principal,
    pub evm_to_icp_fee: Nat,
    pub icp_to_evm_fee: Nat,
    pub operator: Operator,
}
