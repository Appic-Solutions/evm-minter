//use std::str::FromStr;

use crate::eth_types::Address;
use crate::numeric::BlockNumber;
use crate::rpc_declarations::Topic;
use crate::state::State;

use super::parser::{LogParser, ReceivedEventsLogParser};
//use super::types::{
//    RECEIVED_DEPLOYED_WRAPPED_ICRC_TOKEN_EVENT_TOPIC,
//    RECEIVED_DEPOSITED_AND_BURNT_TOKENS_EVENT_TOPIC_NEW_CONTRACT,
//    RECEIVED_DEPOSITED_TOKEN_EVENT_TOPIC_OLD_CONTRACT,
//};

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

    fn next_scrape(state: &State) -> Option<Scrape> {
        let mut contract_addresses = state
            .helper_contract_addresses
            .clone()
            .expect("Scraping not activated");

        let last_scraped_block_number = state.last_scraped_block_number;

        // We add native token address as 0;
        //let mut token_contract_addresses =
        //    state.erc20_tokens.alt_keys().cloned().collect::<Vec<_>>();

        if let Some(swap_contract_address) = state.swap_contract_address {
            contract_addresses.push(swap_contract_address);
        }

        // Add native token
        //token_contract_addresses.push(
        //    Address::from_str("0x0000000000000000000000000000000000000000")
        //        .expect("Should not fail converting zero address"),
        //);

        let topics: Vec<_> = vec![
        //Topic::from(vec![
        //    FixedSizeData(RECEIVED_DEPOSITED_AND_BURNT_TOKENS_EVENT_TOPIC_NEW_CONTRACT),
        //    FixedSizeData(RECEIVED_DEPOSITED_TOKEN_EVENT_TOPIC_OLD_CONTRACT),
        //    FixedSizeData(RECEIVED_DEPLOYED_WRAPPED_ICRC_TOKEN_EVENT_TOPIC),
        //])
        ];

        // We add token contract addresses as additional topics to match.
        // It has a disjunction semantics, so it will match if event matches any one of these addresses.
        //topics.push(
        //    token_contract_addresses
        //        .iter()
        //        .map(|address| FixedSizeData(address.into()))
        //        .collect::<Vec<_>>()
        //        .into(),
        //);

        Some(Scrape {
            contract_addresses,
            last_scraped_block_number,
            topics,
        })
    }

    fn update_last_scraped_block_number(state: &mut State, block_number: BlockNumber) {
        state.last_scraped_block_number = block_number;
    }
}
