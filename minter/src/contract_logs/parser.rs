use crate::contract_logs::swap::swap_logs::{ReceivedSwapEvent, RECEIVED_SWAP_EVENT_TOPIC};
use crate::contract_logs::{
    parse_principal_from_slice, EventSource, EventSourceError, LedgerSubaccount,
    ReceivedContractEventError,
};

use crate::eth_types::Address;
use crate::numeric::{BlockNumber, Erc20Value, IcrcValue, Wei};
use crate::rpc_declarations::{Data, FixedSizeData, LogEntry};
use crate::state::read_state;
use candid::Principal;

use super::types::{
    ReceivedBurnEvent, ReceivedErc20Event, ReceivedNativeEvent, ReceivedWrappedIcrcDeployedEvent,
    RECEIVED_DEPLOYED_WRAPPED_ICRC_TOKEN_EVENT_TOPIC,
    RECEIVED_DEPOSITED_AND_BURNT_TOKENS_EVENT_TOPIC_NEW_CONTRACT,
    RECEIVED_DEPOSITED_TOKEN_EVENT_TOPIC_OLD_CONTRACT,
};
use super::ReceivedContractEvent;

/// Parse an deposit log event into a `ReceivedDepositEvent`.
pub trait LogParser {
    fn parse_log(log: LogEntry) -> Result<ReceivedContractEvent, ReceivedContractEventError>;

    /// Parse a list of deposit logs events into a list of `ReceivedDepositEvent`s and a list of errors.
    /// All logs are parsed, even if some of them are invalid.
    fn parse_all_logs(
        logs: Vec<LogEntry>,
    ) -> (Vec<ReceivedContractEvent>, Vec<ReceivedContractEventError>) {
        let (ok, not_ok): (Vec<_>, Vec<_>) = logs
            .into_iter()
            .map(Self::parse_log)
            .partition(Result::is_ok);
        let valid_transactions: Vec<ReceivedContractEvent> =
            ok.into_iter().map(Result::unwrap).collect();

        let errors: Vec<ReceivedContractEventError> =
            not_ok.into_iter().map(Result::unwrap_err).collect();
        (valid_transactions, errors)
    }
}

pub enum ReceivedEventsLogParser {}

impl LogParser for ReceivedEventsLogParser {
    fn parse_log(entry: LogEntry) -> Result<ReceivedContractEvent, ReceivedContractEventError> {
        let (block_number, event_source) = ensure_not_pending(&entry)?;
        ensure_not_removed(&entry, event_source)?;

        let event_signature = entry.topics.first();

        match event_signature {
            Some(&FixedSizeData(RECEIVED_DEPOSITED_TOKEN_EVENT_TOPIC_OLD_CONTRACT)) => {
                // We have 4 indexed topics for all deposit events:
                // The overall event is as follow :
                // DepositLog(
                //     address from_address,
                //     address indexed token,
                //     uint256 indexed amount,
                //     bytes32 indexed principal,
                //     bytes32 subaccount
                // );

                let [from_address_bytes, subaccount_bytes] =
                    parse_data_into_32_byte_words(entry.data, event_source)?;

                let from_address = parse_address(&FixedSizeData(from_address_bytes), event_source)?;
                let subaccount = LedgerSubaccount::from_bytes(subaccount_bytes);

                let token_contract_address = parse_address(&entry.topics[1], event_source)?;

                let principal = parse_principal(&entry.topics[3], event_source)?;

                let value = &entry.topics[2];

                let EventSource {
                    transaction_hash,
                    log_index,
                } = event_source;

                if token_contract_address.is_native_token() {
                    Ok(ReceivedContractEvent::NativeDeposit(ReceivedNativeEvent {
                        transaction_hash,
                        block_number,
                        log_index,
                        from_address,
                        value: Wei::from_be_bytes(value.0),
                        principal,
                        subaccount,
                    }))
                } else {
                    if read_state(|s| s.erc20_tokens.get_alt(&token_contract_address).is_none()) {
                        return Err(ReceivedContractEventError::InvalidEventSource {
                            source: event_source,
                            error: EventSourceError::InvalidEvent(
                                "Deposited Erc20 token is not supported by the minter".to_string(),
                            ),
                        });
                    }

                    Ok(ReceivedContractEvent::Erc20Deposit(ReceivedErc20Event {
                        transaction_hash,
                        block_number,
                        log_index,
                        from_address,
                        value: Erc20Value::from_be_bytes(value.0),
                        principal,
                        erc20_contract_address: token_contract_address,
                        subaccount,
                    }))
                }
            }
            Some(&FixedSizeData(RECEIVED_DEPOSITED_AND_BURNT_TOKENS_EVENT_TOPIC_NEW_CONTRACT)) => {
                let EventSource {
                    transaction_hash,
                    log_index,
                } = event_source;

                //event TokenBurn(
                //      address indexed fromAddress,
                //      uint256 amount,
                //      bytes32 indexed icpRecipient,
                //      address indexed TokenAddress,
                //      bytes32 subaccount
                //  );

                let from_address = parse_address(&entry.topics[1], event_source)?;

                let [amount_bytes, subaccount_bytes] =
                    parse_data_into_32_byte_words(entry.data, event_source)?;

                let burnt_erc20 = parse_address(&entry.topics[3], event_source)?;

                let principal = parse_principal(&entry.topics[2], event_source)?;

                let subaccount = LedgerSubaccount::from_bytes(subaccount_bytes);

                if burnt_erc20.is_native_token() {
                    Ok(ReceivedContractEvent::NativeDeposit(ReceivedNativeEvent {
                        transaction_hash,
                        block_number,
                        log_index,
                        from_address,
                        value: Wei::from_be_bytes(amount_bytes),
                        principal,
                        subaccount,
                    }))
                } else if read_state(|s| s.erc20_tokens.get_alt(&burnt_erc20).is_some()) {
                    Ok(ReceivedContractEvent::Erc20Deposit(ReceivedErc20Event {
                        transaction_hash,
                        block_number,
                        log_index,
                        from_address,
                        value: Erc20Value::from_be_bytes(amount_bytes),
                        principal,
                        erc20_contract_address: burnt_erc20,
                        subaccount,
                    }))
                } else if let Some(icrc_token_principal) = read_state(|s| {
                    s.find_icp_token_ledger_id_by_wrapped_erc20_address(&burnt_erc20)
                }) {
                    Ok(ReceivedContractEvent::WrappedIcrcBurn(ReceivedBurnEvent {
                        transaction_hash,
                        block_number,
                        log_index,
                        from_address,
                        value: IcrcValue::from_be_bytes(amount_bytes),
                        principal,
                        wrapped_erc20_contract_address: burnt_erc20,
                        subaccount,
                        icrc_token_principal,
                    }))
                } else {
                    Err(ReceivedContractEventError::InvalidEventSource {
                        source: event_source,
                        error: EventSourceError::InvalidEvent(
                            "Burnt erc20 token is not supported by minter.".to_string(),
                        ),
                    })
                }
            }
            Some(&FixedSizeData(RECEIVED_DEPLOYED_WRAPPED_ICRC_TOKEN_EVENT_TOPIC)) => {
                let EventSource {
                    transaction_hash,
                    log_index,
                } = event_source;

                //event WrappedTokenDeployed(
                //    bytes32 indexed baseToken,
                //    address indexed wrappedERC20
                //);
                let base_token = parse_principal(&entry.topics[1], event_source)?;

                let deployed_wrapped_erc20 = parse_address(&entry.topics[2], event_source)?;

                Ok(ReceivedContractEvent::WrappedIcrcDeployed(
                    ReceivedWrappedIcrcDeployedEvent {
                        transaction_hash,
                        block_number,
                        log_index,
                        base_token,
                        deployed_wrapped_erc20,
                    },
                ))
            }
            Some(&FixedSizeData(RECEIVED_SWAP_EVENT_TOPIC)) => {
                let EventSource {
                    transaction_hash,
                    log_index,
                } = event_source;
                // event SwapExecuted(
                // address user,
                // bytes32 indexed recipient,
                // address indexed tokenIn,
                // address indexed tokenOut,
                // uint256 amountIn,
                // uint256 amountOut,
                // bool bridgeToMinter,
                // bytes encodedData
                //);
                let recipient = entry.topics[1].clone();
                let token_in = parse_address(&entry.topics[2], event_source)?;
                let token_out = parse_address(&entry.topics[3], event_source)?;
                let (fixed_words, encoded_swap_data) =
                    parse_swap_executed_data(entry.data, event_source)?;
                let from_address = parse_address(&FixedSizeData(fixed_words[0]), event_source)?;
                let amount_in = Erc20Value::from_be_bytes(fixed_words[1]);
                let amount_out = Erc20Value::from_be_bytes(fixed_words[2]);
                let bridge_word = fixed_words[3];
                let bridged_to_minter = if bridge_word == [0u8; 32] {
                    false
                } else if bridge_word[31] == 1 && bridge_word[0..31].iter().all(|&b| b == 0) {
                    true
                } else {
                    return Err(ReceivedContractEventError::InvalidEventSource {
                        source: event_source,
                        error: EventSourceError::InvalidEvent(
                            "Invalid bool encoding for bridgeToMinter".to_string(),
                        ),
                    });
                };
                if !bridged_to_minter {
                    Err(ReceivedContractEventError::SameChainSwap)
                } else {
                    Ok(ReceivedContractEvent::ReceivedSwapOrder(
                        ReceivedSwapEvent {
                            transaction_hash,
                            block_number,
                            log_index,
                            from_address,
                            recipient,
                            token_in,
                            token_out,
                            amount_in,
                            amount_out,
                            bridged_to_minter,
                            encoded_swap_data,
                        },
                    ))
                }
            }
            Some(_) => Err(ReceivedContractEventError::InvalidEventSource {
                source: event_source,
                error: EventSourceError::InvalidEvent("Invalid event signature".to_string()),
            }),
            None => Err(ReceivedContractEventError::InvalidEventSource {
                source: event_source,
                error: EventSourceError::InvalidEvent("Invalid event signature".to_string()),
            }),
        }
    }
}

fn ensure_not_pending(
    entry: &LogEntry,
) -> Result<(BlockNumber, EventSource), ReceivedContractEventError> {
    let _block_hash = entry
        .block_hash
        .ok_or(ReceivedContractEventError::PendingLogEntry)?;
    let block_number = entry
        .block_number
        .ok_or(ReceivedContractEventError::PendingLogEntry)?;
    let transaction_hash = entry
        .transaction_hash
        .ok_or(ReceivedContractEventError::PendingLogEntry)?;
    let _transaction_index = entry
        .transaction_index
        .ok_or(ReceivedContractEventError::PendingLogEntry)?;
    let log_index = entry
        .log_index
        .ok_or(ReceivedContractEventError::PendingLogEntry)?;
    Ok((
        block_number,
        EventSource {
            transaction_hash,
            log_index,
        },
    ))
}

fn ensure_not_removed(
    entry: &LogEntry,
    event_source: EventSource,
) -> Result<(), ReceivedContractEventError> {
    if entry.removed {
        return Err(ReceivedContractEventError::InvalidEventSource {
            source: event_source,
            error: EventSourceError::InvalidEvent(
                "this event has been removed from the chain".to_string(),
            ),
        });
    }
    Ok(())
}

//fn ensure_topics<P>(
//    entry: &LogEntry,
//    predicate: P,
//    event_source: EventSource,
//) -> Result<(), ReceivedContractEventError>
//where
//    P: FnOnce(&[FixedSizeData]) -> bool,
//{
//    if !predicate(&entry.topics) {
//        return Err(ReceivedContractEventError::InvalidEventSource {
//            source: event_source,
//            error: EventSourceError::InvalidEvent("Invalid topics".to_string()),
//        });
//    }
//    Ok(())
//}

fn parse_address(
    address: &FixedSizeData,
    event_source: EventSource,
) -> Result<Address, ReceivedContractEventError> {
    Address::try_from(&address.0).map_err(|err| ReceivedContractEventError::InvalidEventSource {
        source: event_source,
        error: EventSourceError::InvalidEvent(format!("Invalid address in log entry: {err}")),
    })
}

fn parse_principal(
    principal: &FixedSizeData,
    event_source: EventSource,
) -> Result<Principal, ReceivedContractEventError> {
    parse_principal_from_slice(&principal.0).map_err(|_err| {
        ReceivedContractEventError::InvalidEventSource {
            source: event_source,
            error: EventSourceError::InvalidPrincipal {
                invalid_principal: principal.clone(),
            },
        }
    })
}

fn parse_data_into_32_byte_words<const N: usize>(
    data: Data,
    event_source: EventSource,
) -> Result<[[u8; 32]; N], ReceivedContractEventError> {
    let data = data.0;
    if data.len() != 32 * N {
        return Err(ReceivedContractEventError::InvalidEventSource {
            source: event_source,
            error: EventSourceError::InvalidEvent(format!(
                "Expected {} bytes, got {}",
                32 * N,
                data.len()
            )),
        });
    }
    let mut result = Vec::with_capacity(N);
    for chunk in data.chunks_exact(32) {
        let mut word = [0; 32];
        word.copy_from_slice(chunk);
        result.push(word);
    }
    Ok(result.try_into().unwrap())
}

// Helper function to convert a 32-byte big-endian uint256 to usize.
// Assumes the value fits in usize; errors if too large or non-zero high bytes.
fn bytes32_to_usize(
    bytes: [u8; 32],
    event_source: EventSource,
) -> Result<usize, ReceivedContractEventError> {
    if bytes[0..24].iter().any(|&x| x != 0) {
        return Err(ReceivedContractEventError::InvalidEventSource {
            source: event_source,
            error: EventSourceError::InvalidEvent(
                "Value too large for usize (high bytes non-zero)".to_string(),
            ),
        });
    }
    let mut be_bytes = [0u8; 8];
    be_bytes.copy_from_slice(&bytes[24..32]);
    let val = u64::from_be_bytes(be_bytes);
    if val > usize::MAX as u64 {
        return Err(ReceivedContractEventError::InvalidEventSource {
            source: event_source,
            error: EventSourceError::InvalidEvent("Value exceeds usize::MAX".to_string()),
        });
    }
    Ok(val as usize)
}

// Specific function for parsing the SwapExecuted event data.
// Returns the four fixed 32-byte words (user, amountIn, amountOut, bridgeToMinter)
// and the variable-length encodedData as Data.
fn parse_swap_executed_data(
    data: Data,
    event_source: EventSource,
) -> Result<([[u8; 32]; 4], Data), ReceivedContractEventError> {
    let bytes = data.0;
    // Minimum length: head (160 bytes) + length word (32 bytes) for empty bytes.
    if bytes.len() < 192 {
        return Err(ReceivedContractEventError::InvalidEventSource {
            source: event_source,
            error: EventSourceError::InvalidEvent(format!(
                "Data too short: expected at least 192 bytes, got {}",
                bytes.len()
            )),
        });
    }
    // Extract the fixed parts.
    let mut fixed_words = [[0u8; 32]; 4];
    fixed_words[0].copy_from_slice(&bytes[0..32]); // user (address)
    fixed_words[1].copy_from_slice(&bytes[32..64]); // amountIn (uint256)
    fixed_words[2].copy_from_slice(&bytes[64..96]); // amountOut (uint256)
    fixed_words[3].copy_from_slice(&bytes[96..128]); // bridgeToMinter (bool)

    // Extract the offset to encodedData (should be 160 / 0xa0).
    let offset_bytes: [u8; 32] = bytes[128..160].try_into().unwrap();
    let offset = bytes32_to_usize(offset_bytes, event_source)?;
    // For this event, offset should always be 160 since there are 4 static params before the dynamic one.
    if offset != 160 {
        return Err(ReceivedContractEventError::InvalidEventSource {
            source: event_source,
            error: EventSourceError::InvalidEvent(format!(
                "Unexpected offset for encodedData: expected 160, got {offset}"
            )),
        });
    }

    // Extract the length of encodedData.
    let len_bytes: [u8; 32] = bytes[offset..offset + 32].try_into().unwrap();
    let len = bytes32_to_usize(len_bytes, event_source)?;

    // Calculate positions.
    let data_start = offset + 32;
    let min_required_len = data_start + len;
    if bytes.len() < min_required_len {
        return Err(ReceivedContractEventError::InvalidEventSource {
            source: event_source,
            error: EventSourceError::InvalidEvent(format!(
                "Data too short for encodedData length {}: expected at least {} bytes, got {}",
                len,
                min_required_len,
                bytes.len()
            )),
        });
    }

    // Calculate padded length and check total data length matches.
    let padded_len = ((len + 31) / 32) * 32;
    let expected_total_len = data_start + padded_len;
    if bytes.len() != expected_total_len {
        return Err(ReceivedContractEventError::InvalidEventSource {
            source: event_source,
            error: EventSourceError::InvalidEvent(format!(
                "Invalid total data length: expected {} bytes (with padding), got {}",
                expected_total_len,
                bytes.len()
            )),
        });
    }

    // Optional: Verify padding bytes are zero.
    for i in len..padded_len {
        if bytes[data_start + i] != 0 {
            return Err(ReceivedContractEventError::InvalidEventSource {
                source: event_source,
                error: EventSourceError::InvalidEvent("Non-zero bytes in padding".to_string()),
            });
        }
    }

    // Extract the encodedData (exact length, without padding).
    let encoded_data_vec = bytes[data_start..data_start + len].to_vec();

    Ok((fixed_words, Data(encoded_data_vec)))
}
