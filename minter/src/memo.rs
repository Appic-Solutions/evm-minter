#[cfg(test)]
mod tests;
use crate::contract_logs::ReceivedContractEvent;
use crate::erc20::ERC20TokenSymbol;
use crate::eth_types::Address;
use crate::numeric::Erc20Value;
use crate::state::transactions::ReimbursementRequest;
use candid::Principal;
use icrc_ledger_types::icrc1::transfer::Memo;
use minicbor::{Decode, Encode, Encoder};

/// Encodes minter memo as a binary blob.
fn encode<T: minicbor::Encode<()>>(t: &T) -> Vec<u8> {
    let mut encoder = Encoder::new(Vec::new());
    encoder.encode(t).expect("minicbor encoding failed");
    encoder.into_writer()
}

#[derive(Decode, Encode, Debug, Eq, PartialEq)]
pub enum MintMemo {
    #[n(0)]
    /// The minter received some ETH or ERC20 token.
    Convert {
        #[n(0)]
        /// The sender of the ETH or ERC20 token.
        from_address: Address,
    },
    #[n(1)]
    ReimburseTransaction {
        #[n(0)]
        /// The id corresponding to the withdrawal request.
        withdrawal_id: u64,
    },
    /// The minter failed to process a withdrawal request,
    /// so no transaction was issued, but some reimbursement was made.
    #[n(2)]
    ReimburseWithdrawal {
        #[n(0)]
        /// The id corresponding to the withdrawal request.
        withdrawal_id: u64,
    },
}

impl From<MintMemo> for Memo {
    fn from(value: MintMemo) -> Self {
        Memo::from(encode(&value))
    }
}

#[derive(Decode, Encode, Debug, Eq, PartialEq, Clone)]
pub enum BurnMemo {
    #[n(0)]
    /// The minter processed a withdrawal request.
    Convert {
        #[n(0)]
        /// The destination of the withdrawal request.
        to_address: Address,
    },
    /// The minter processed a ERC20 withdrawal request
    /// and that burn pays the transaction fee.
    #[n(1)]
    Erc20GasFee {
        /// ckERC20 token symbol of the withdrawal request.
        #[n(0)]
        erc20_token_symbol: ERC20TokenSymbol,

        /// The amount of the ERC20 withdrawal request.
        #[n(1)]
        erc20_withdrawal_amount: Erc20Value,

        /// The destination of the withdrawal request.
        #[n(2)]
        to_address: Address,
    },

    /// The minter processed a ERC20 withdrawal request.
    #[n(2)]
    Erc20Convert {
        /// native ledger burn index identifying the burn to pay for the transaction fee.
        #[n(0)]
        erc20_withdrawal_id: u64,

        /// The destination of the withdrawal request.
        #[n(1)]
        to_address: Address,
    },

    #[n(3)]
    /// The minter processed a WrapIcrc request
    /// and that burn pays the transaction fee.
    WrapIcrcGasFee {
        /// icrc base token to be wrapped.
        #[cbor(n(0), with = "crate::cbor::principal")]
        wrapped_icrc_base: Principal,

        /// The amount of the Icrc wrapped request.
        #[n(1)]
        wrap_amount: Erc20Value,

        /// The destination of the wrapped token.
        #[n(2)]
        to_address: Address,
    },

    #[n(4)]
    /// Locked icrc token to be wrapped on the evm side
    /// intentionally kept short to prevent ledger memo size limit specially when it comes to ICP
    /// ledger
    IcrcLocked {
        /// The destination of the withdrawal request.
        #[n(1)]
        to_address: Address,
    },
}

impl From<BurnMemo> for Memo {
    fn from(value: BurnMemo) -> Self {
        Memo::from(encode(&value))
    }
}

impl From<&ReceivedContractEvent> for Memo {
    fn from(event: &ReceivedContractEvent) -> Self {
        //todo!()
        match event {
            ReceivedContractEvent::NativeDeposit(received_native_event) => MintMemo::Convert {
                from_address: received_native_event.from_address,
            },
            ReceivedContractEvent::Erc20Deposit(received_erc20_event) => MintMemo::Convert {
                from_address: received_erc20_event.from_address,
            },
            ReceivedContractEvent::WrappedIcrcBurn(received_burn_event) => MintMemo::Convert {
                from_address: received_burn_event.from_address,
            },
            ReceivedContractEvent::WrappedIcrcDeployed(_received_wrapped_icrc_deployed_event) => {
                panic!("Bug: this event is not mintable")
            }
            ReceivedContractEvent::ReceivedSwapOrder(received_swap_event) => MintMemo::Convert {
                from_address: received_swap_event.from_address,
            },
        }
        .into()
    }
}

impl From<ReimbursementRequest> for MintMemo {
    fn from(reimbursement_request: ReimbursementRequest) -> Self {
        match reimbursement_request.transaction_hash {
            Some(_tx_hash) => MintMemo::ReimburseTransaction {
                withdrawal_id: reimbursement_request.ledger_burn_index.get(),
            },
            None => MintMemo::ReimburseWithdrawal {
                withdrawal_id: reimbursement_request.ledger_burn_index.get(),
            },
        }
    }
}

impl From<ReimbursementRequest> for Memo {
    fn from(reimbursement_request: ReimbursementRequest) -> Self {
        Memo::from(MintMemo::from(reimbursement_request))
    }
}
