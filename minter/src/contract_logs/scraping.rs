use std::str::FromStr;

use crate::eth_types::Address;
use crate::numeric::BlockNumber;
use crate::rpc_declarations::{FixedSizeData, Topic};
use crate::state::State;

use super::new_contract::{RECEIVED_BURNT_TOKEN_EVENT, RECEIVED_WRAPPED_ICP_DEPLOYED_EVENT};
use super::old_contract::RECEIVED_DEPOSITED_TOKEN_EVENT_TOPIC;
use super::parser::{LogParser, ReceivedEventsLogParser};

pub struct Scrape {
    pub contract_addresses: Vec<Address>,
    pub last_scraped_block_number: BlockNumber,
    pub topics: Vec<Topic>,
}

/// Trait for managing log scraping.
pub trait LogScraping {
    /// The parser type that defines how to parse logs found by this log scraping.
    type Parser: LogParser;

    fn next_scrape(state: &State) -> Option<Scrape>;
    fn update_last_scraped_block_number(state: &mut State, block_number: BlockNumber);
}

pub enum ReceivedEventsLogScraping {}

impl LogScraping for ReceivedEventsLogScraping {
    type Parser = ReceivedEventsLogParser;

    // TODO: Add contract addresses
    fn next_scrape(state: &State) -> Option<Scrape> {
        let contract_address = state
            .helper_contract_address
            .expect("Scraping not activated");

        let last_scraped_block_number = state.last_scraped_block_number;

        // We add native token address as 0;
        let mut token_contract_addresses =
            state.erc20_tokens.alt_keys().cloned().collect::<Vec<_>>();

        // Add native token
        token_contract_addresses.push(
            Address::from_str("0x0000000000000000000000000000000000000000")
                .expect("Should not fail converting zero address"),
        );

        let mut topics: Vec<_> = vec![
            Topic::from(FixedSizeData(RECEIVED_DEPOSITED_TOKEN_EVENT_TOPIC)),
            Topic::from(FixedSizeData(RECEIVED_BURNT_TOKEN_EVENT)),
            Topic::from(FixedSizeData(RECEIVED_WRAPPED_ICP_DEPLOYED_EVENT)),
        ];
        // We add token contract addresses as additional topics to match.
        // It has a disjunction semantics, so it will match if event matches any one of these addresses.
        topics.push(
            token_contract_addresses
                .iter()
                .map(|address| FixedSizeData(address.into()))
                .collect::<Vec<_>>()
                .into(),
        );

        Some(Scrape {
            contract_addresses: vec![],
            last_scraped_block_number,
            topics,
        })
    }

    fn update_last_scraped_block_number(state: &mut State, block_number: BlockNumber) {
        state.last_scraped_block_number = block_number;
    }
}
