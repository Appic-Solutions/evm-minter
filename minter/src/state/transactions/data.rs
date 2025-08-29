use std::str::FromStr;

use crate::{eth_types::Address, numeric::Erc20Value, rpc_declarations::Data};
use alloy::primitives::{Address as AlloyAddress, Bytes, FixedBytes, U256};
use alloy::sol_types::{SolCall, SolType};
use minicbor::{Decode, Encode};
//use alloy_primitives::{Address as AlloyAddress, FixedBytes,U256};

// Existing selectors
const ERC_20_TRANSFER_FUNCTION_SELECTOR: [u8; 4] = hex_literal::hex!("a9059cbb");
const ERC_20_APPROVE_FUNCTION_SELECTOR: [u8; 4] = hex_literal::hex!("095ea7b3");
const EXECUTE_SWAP_FUNCTION_SELECTOR: [u8; 4] = hex_literal::hex!("13178b7a");

// Command enum
#[derive(Clone, Copy, Debug, Eq, PartialEq, Encode, Decode)]
pub enum Command {
    #[n(0)]
    V2Swap,
    #[n(1)]
    V3Single,
    #[n(2)]
    V3Multi,
    #[n(3)]
    WrapEth,
    #[n(4)]
    UnwrapEth,
}

impl Command {
    pub fn to_u8(self) -> u8 {
        match self {
            Command::V2Swap => 0,
            Command::V3Single => 1,
            Command::V3Multi => 2,
            Command::WrapEth => 3,
            Command::UnwrapEth => 4,
        }
    }
    pub fn from_u8(val: u8) -> Result<Self, String> {
        match val {
            0 => Ok(Command::V2Swap),
            1 => Ok(Command::V3Single),
            2 => Ok(Command::V3Multi),
            3 => Ok(Command::WrapEth),
            4 => Ok(Command::UnwrapEth),
            _ => Err("Invalid Command".to_string()),
        }
    }
}

// Helper to encode usize as uint256
fn encode_u256(val: usize) -> [u8; 32] {
    let mut bytes = [0u8; 32];
    bytes[24..].copy_from_slice(&(val as u64).to_be_bytes());
    bytes
}

// Helper to encode bool as uint256
fn encode_bool(val: bool) -> [u8; 32] {
    let mut bytes = [0u8; 32];
    bytes[31] = if val { 1 } else { 0 };
    bytes
}

// Helper to decode uint256 to usize
fn decode_usize(bytes: &[u8; 32]) -> Result<usize, String> {
    let val = u64::from_be_bytes(bytes[24..32].try_into().unwrap());
    usize::try_from(val).map_err(|_| "Value too large".to_string())
}

// Define the executeSwap function signature using Alloy's sol! macro
alloy::sol! {
    function executeSwap(
        uint8[] calldata commands,
        bytes[] calldata data,
        address tokenIn,
        uint256 amountIn,
        uint256 minAmountOut,
        uint256 deadline,
        bytes calldata encodedData,
        bytes32 recipient,
        bool bridgeToMinter
    );
}

// Extend enum
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum TransactionCallData {
    Erc20Transfer {
        to: Address,
        value: Erc20Value,
    },
    Erc20Approve {
        spender: Address,
        value: Erc20Value,
    },
    ExecuteSwap {
        commands: Vec<Command>,
        data: Vec<Data>,
        token_in: Address,
        amount_in: Erc20Value,
        min_amount_out: Erc20Value,
        deadline: Erc20Value,
        encoded_data: Data,
        recipient: Address,
        bridge_to_minter: bool,
    },
}

impl TransactionCallData {
    pub fn encode(&self) -> Vec<u8> {
        match self {
            TransactionCallData::Erc20Transfer { to, value } => {
                let mut data = Vec::with_capacity(68);
                data.extend(ERC_20_TRANSFER_FUNCTION_SELECTOR);
                data.extend(<[u8; 32]>::from(to));
                data.extend(value.to_be_bytes());
                data
            }
            TransactionCallData::Erc20Approve { spender, value } => {
                let mut data = Vec::with_capacity(68);
                data.extend(ERC_20_APPROVE_FUNCTION_SELECTOR);
                data.extend(<[u8; 32]>::from(spender));
                data.extend(value.to_be_bytes());
                data
            }
            TransactionCallData::ExecuteSwap {
                commands,
                data,
                token_in,
                amount_in,
                min_amount_out,
                deadline,
                encoded_data,
                recipient,
                bridge_to_minter,
            } => {
                // Convert to Alloy types
                let commands: Vec<u8> = commands.iter().map(|c| c.to_u8()).collect();
                let data: Vec<Bytes> = data.iter().map(|d| Bytes::copy_from_slice(&d.0)).collect();
                let token_in = AlloyAddress::from_slice(&token_in.into_bytes());
                let amount_in = U256::from_be_bytes(amount_in.to_be_bytes());
                let min_amount_out = U256::from_be_bytes(min_amount_out.to_be_bytes());
                let deadline = U256::from_be_bytes(deadline.to_be_bytes());
                let recipient = FixedBytes::<32>::from(&recipient.into());
                let encoded_data = Bytes::copy_from_slice(&encoded_data.0);

                // Create the SolCall struct
                let call = executeSwapCall {
                    commands,
                    data,
                    tokenIn: token_in,
                    amountIn: amount_in,
                    minAmountOut: min_amount_out,
                    deadline,
                    encodedData: encoded_data,
                    recipient,
                    bridgeToMinter: *bridge_to_minter,
                };

                // Encode using Alloy
                call.abi_encode()
            }
        }
    }

    pub fn decode<T: AsRef<[u8]>>(data: T) -> Result<Self, String> {
        let data = data.as_ref();
        if data.len() < 4 {
            return Err("Data too short".to_string());
        }
        let selector: [u8; 4] = data[0..4].try_into().unwrap();
        match selector {
            ERC_20_TRANSFER_FUNCTION_SELECTOR => {
                if data.len() != 68 {
                    return Err("Invalid data length".to_string());
                }
                let address = <[u8; 32]>::try_from(&data[4..36]).unwrap();
                let to = Address::try_from(&address).unwrap();

                let value = <[u8; 32]>::try_from(&data[36..]).unwrap();
                let value = Erc20Value::from_be_bytes(value);

                Ok(TransactionCallData::Erc20Transfer { to, value })
            }
            ERC_20_APPROVE_FUNCTION_SELECTOR => {
                if data.len() != 68 {
                    return Err("Invalid data length".to_string());
                }
                let address = <[u8; 32]>::try_from(&data[4..36]).unwrap();
                let spender = Address::try_from(&address).unwrap();

                let value = <[u8; 32]>::try_from(&data[36..]).unwrap();
                let value = Erc20Value::from_be_bytes(value);

                Ok(TransactionCallData::Erc20Approve { spender, value })
            }
            EXECUTE_SWAP_FUNCTION_SELECTOR => {
                // Decode using Alloy
                let call = executeSwapCall::abi_decode(&data[4..], true)
                    .map_err(|e| format!("Decode error: {e}"))?;

                // Convert back to your types
                let commands = call
                    .commands
                    .into_iter()
                    .map(Command::from_u8)
                    .collect::<Result<Vec<_>, _>>()?;

                let token_in = Address::from_str(&call.tokenIn.to_string()).map_err(|e| {
                    format!("Failed to decode alloy address into local address {e}")
                })?;

                let recipient = Address::from_str(&AlloyAddress::from(call.recipient).to_string())
                    .map_err(|e| {
                        format!("Failed to decode alloy Bytes<32> into local address {e}")
                    })?;

                let data = call.data.iter().map(|d| Data(d.to_vec())).collect();
                let amount_in = Erc20Value::from_be_bytes(call.amountIn.to_be_bytes());
                let min_amount_out = Erc20Value::from_be_bytes(call.minAmountOut.to_be_bytes());
                let deadline = Erc20Value::from_be_bytes(call.deadline.to_be_bytes());
                let recipient;
                let encoded_data = Data(call.encodedData.to_vec());

                Ok(TransactionCallData::ExecuteSwap {
                    commands,
                    data,
                    token_in,
                    amount_in,
                    min_amount_out,
                    deadline,
                    encoded_data,
                    recipient,
                    bridge_to_minter: call.bridgeToMinter,
                })
            }
            _ => Err(format!("Unknown selector 0x{}", hex::encode(selector))),
        }
    }
}
