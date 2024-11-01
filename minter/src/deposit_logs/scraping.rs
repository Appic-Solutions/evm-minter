use crate::deposit_logs::{
    LogParser, ReceivedDepositLogParser, RECEIVED_DEPOSITED_TOKEN_EVENT_TOPIC,
};
use crate::eth_types::Address;
use crate::numeric::BlockNumber;
use crate::rpc_declrations::{FixedSizeData, Topic};
use crate::state::State;

pub struct Scrape {
    pub contract_address: Address,
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

pub enum ReceivedDepositLogScraping {}

impl LogScraping for ReceivedDepositLogScraping {
    type Parser = ReceivedDepositLogParser;

    fn next_scrape(state: &State) -> Option<Scrape> {
        let contract_address = state.helper_contract_address.unwrap();
        let last_scraped_block_number = state.last_scraped_block_number;

        let token_contract_addresses = state.erc20_tokens.alt_keys().cloned().collect::<Vec<_>>();
        let mut topics: Vec<_> = vec![Topic::from(FixedSizeData(
            RECEIVED_DEPOSITED_TOKEN_EVENT_TOPIC,
        ))];
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
            contract_address,
            last_scraped_block_number,
            topics,
        })
    }

    fn update_last_scraped_block_number(state: &mut State, block_number: BlockNumber) {
        state.last_scraped_block_number = block_number;
    }
}
