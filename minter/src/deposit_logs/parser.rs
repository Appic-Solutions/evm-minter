use crate::deposit_logs::{
    parse_principal_from_slice, EventSource, EventSourceError, LedgerSubaccount,
    ReceivedDepositEvent, ReceivedDepositEventError, ReceivedErc20Event, ReceivedNativeEvent,
};

use crate::eth_types::Address;
use crate::numeric::{BlockNumber, Erc20Value, Wei};
use crate::rpc_declarations::{Data, FixedSizeData, LogEntry};
use candid::Principal;

use super::RECEIVED_DEPOSITED_TOKEN_EVENT_TOPIC;

/// Parse an deposit log event into a `ReceivedDepositEvent`.
pub trait LogParser {
    fn parse_log(log: LogEntry) -> Result<ReceivedDepositEvent, ReceivedDepositEventError>;

    /// Parse a list of deposit logs events into a list of `ReceivedDepositEvent`s and a list of errors.
    /// All logs are parsed, even if some of them are invalid.
    fn parse_all_logs(
        logs: Vec<LogEntry>,
    ) -> (Vec<ReceivedDepositEvent>, Vec<ReceivedDepositEventError>) {
        let (ok, not_ok): (Vec<_>, Vec<_>) = logs
            .into_iter()
            .map(Self::parse_log)
            .partition(Result::is_ok);
        let valid_transactions: Vec<ReceivedDepositEvent> =
            ok.into_iter().map(Result::unwrap).collect();
        let errors: Vec<ReceivedDepositEventError> =
            not_ok.into_iter().map(Result::unwrap_err).collect();
        (valid_transactions, errors)
    }
}

pub enum ReceivedDepositLogParser {}

impl LogParser for ReceivedDepositLogParser {
    fn parse_log(entry: LogEntry) -> Result<ReceivedDepositEvent, ReceivedDepositEventError> {
        let (block_number, event_source) = ensure_not_pending(&entry)?;
        ensure_not_removed(&entry, event_source)?;

        ensure_topics(
            &entry,
            |topics| {
                topics.len() == 4
                    && topics.first() == Some(&FixedSizeData(RECEIVED_DEPOSITED_TOKEN_EVENT_TOPIC))
            },
            event_source,
        )?;
        let [from_address_bytes, subaccount_bytes] =
            parse_data_into_32_byte_words(entry.data, event_source)?;

        let from_address = parse_address(&FixedSizeData(from_address_bytes), event_source)?;
        let subaccount = LedgerSubaccount::from_bytes(subaccount_bytes);

        // We have 4 indexed topics for all deposit events:
        // The overal event is as follow :
        // DepositLog(
        //     address from_address,
        //     address indexed token,
        //     uint256 indexed amount,
        //     bytes32 indexed principal,
        //     bytes32 subaccount
        // );
        // Indexed topics are as follow
        // (
        //     Event Topic = RECEIVED_DEPOSITED_TOKEN_EVENT_TOPIC,
        //     Indexed contract_address of the token(in case of native token its 0x000000000000000000000000000),
        //     Indexed amount of token(value),
        //     Indexed principalId
        // );

        let token_contract_address = parse_address(&entry.topics[1], event_source)?;

        let principal = parse_principal(&entry.topics[3], event_source)?;

        let value = &entry.topics[2];

        let EventSource {
            transaction_hash,
            log_index,
        } = event_source;

        match token_contract_address.is_native_token() {
            true => Ok(ReceivedDepositEvent::Native(ReceivedNativeEvent {
                transaction_hash,
                block_number,
                log_index,
                from_address,
                value: Wei::from_be_bytes(value.0),
                principal,
                subaccount,
            })),
            false => Ok(ReceivedDepositEvent::Erc20(ReceivedErc20Event {
                transaction_hash,
                block_number,
                log_index,
                from_address,
                value: Erc20Value::from_be_bytes(value.0),
                principal,
                erc20_contract_address: token_contract_address,
                subaccount,
            })),
        }
    }
}

fn ensure_not_pending(
    entry: &LogEntry,
) -> Result<(BlockNumber, EventSource), ReceivedDepositEventError> {
    let _block_hash = entry
        .block_hash
        .ok_or(ReceivedDepositEventError::PendingLogEntry)?;
    let block_number = entry
        .block_number
        .ok_or(ReceivedDepositEventError::PendingLogEntry)?;
    let transaction_hash = entry
        .transaction_hash
        .ok_or(ReceivedDepositEventError::PendingLogEntry)?;
    let _transaction_index = entry
        .transaction_index
        .ok_or(ReceivedDepositEventError::PendingLogEntry)?;
    let log_index = entry
        .log_index
        .ok_or(ReceivedDepositEventError::PendingLogEntry)?;
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
) -> Result<(), ReceivedDepositEventError> {
    if entry.removed {
        return Err(ReceivedDepositEventError::InvalidEventSource {
            source: event_source,
            error: EventSourceError::InvalidEvent(
                "this event has been removed from the chain".to_string(),
            ),
        });
    }
    Ok(())
}

fn ensure_topics<P>(
    entry: &LogEntry,
    predicate: P,
    event_source: EventSource,
) -> Result<(), ReceivedDepositEventError>
where
    P: FnOnce(&[FixedSizeData]) -> bool,
{
    if !predicate(&entry.topics) {
        return Err(ReceivedDepositEventError::InvalidEventSource {
            source: event_source,
            error: EventSourceError::InvalidEvent("Invalid topics".to_string()),
        });
    }
    Ok(())
}

fn parse_address(
    address: &FixedSizeData,
    event_source: EventSource,
) -> Result<Address, ReceivedDepositEventError> {
    Address::try_from(&address.0).map_err(|err| ReceivedDepositEventError::InvalidEventSource {
        source: event_source,
        error: EventSourceError::InvalidEvent(format!("Invalid address in log entry: {}", err)),
    })
}

fn parse_principal(
    principal: &FixedSizeData,
    event_source: EventSource,
) -> Result<Principal, ReceivedDepositEventError> {
    parse_principal_from_slice(&principal.0).map_err(|_err| {
        ReceivedDepositEventError::InvalidEventSource {
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
) -> Result<[[u8; 32]; N], ReceivedDepositEventError> {
    let data = data.0;
    if data.len() != 32 * N {
        return Err(ReceivedDepositEventError::InvalidEventSource {
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
