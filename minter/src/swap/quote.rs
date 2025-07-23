//use hex::FromHexError;
//use rlp::{DecoderError, Rlp, RlpDecodable};
//use thiserror::Error;
//
//#[derive(Error, Debug)]
//enum DecodeError {
//    #[error("Missing '0x' prefix")]
//    MissingPrefix,
//    #[error("Hex decoding error: {0}")]
//    HexError(#[from] FromHexError),
//    #[error("RLP decoding error: {0}")]
//    RlpError(#[from] DecoderError),
//}
//
//#[derive(Debug)]
//struct CrossChainQuoteData {
//    total_amount_in: String,
//    total_amount_out: String,
//    steps: Vec<CrossChainStep>,
//}
//
//#[derive(Debug)]
//struct CrossChainStep {
//    step: u32,
//    chain: String,
//    chain_id: String,
//    quote: Quote,
//}
//
//#[derive(Debug)]
//enum Quote {
//    SwapRoute(SwapRoute),
//    ICPQuoteResponse(ICPQuoteResponse),
//}
//
//#[derive(Debug)]
//struct SwapRoute {
//    route: Vec<PoolHop>,
//    path: Vec<String>,
//    protocol: String,
//    amount_in: String,
//    amount_out: String,
//    execution_price: Option<String>,
//    price_impact: Option<String>,
//    score: u32,
//    trade_type: Option<String>,
//    gas_limit: Option<String>,
//    gas_limit_unit: Option<String>,
//    max_gas_fee: Option<String>,
//    max_gas_fee_unit: Option<String>,
//}
//
//#[derive(Debug)]
//struct ICPQuoteResponse {
//    token_in: String,
//    token_out: String,
//    amount_in: String,
//    amount_out: String,
//    execution_price: Option<String>,
//    price_impact: Option<String>,
//    route_string: Option<String>,
//    route: Vec<PoolHop>,
//    path: Vec<String>,
//    score: u32,
//}
//
//#[derive(Debug)]
//struct PoolHop {
//    protocol: String,
//    fee: String,
//    sell_token: String,
//    buy_token: String,
//    pool_address: String,
//}
//
//#[derive(RlpDecodable)]
//struct RlpQuoteData {
//    total_amount_in: String,
//    total_amount_out: String,
//    steps: Vec<RlpStep>,
//}
//
//#[derive(RlpDecodable)]
//struct RlpStep {
//    step: String,
//    chain: String,
//    chain_id: String,
//    quote: Vec<Rlp>,
//}
//
//fn decode_pool_hop(rlp: Rlp) -> Result<PoolHop, DecoderError> {
//    if !rlp.is_list() || rlp.item_count()? != 5 {
//        return Err(DecoderError::RlpIncorrectListLen);
//    }
//    Ok(PoolHop {
//        protocol: rlp.at(0)?.as_val()?,
//        fee: rlp.at(1)?.as_val()?,
//        sell_token: rlp.at(2)?.as_val()?,
//        buy_token: rlp.at(3)?.as_val()?,
//        pool_address: rlp.at(4)?.as_val()?,
//    })
//}
//
//fn decode_swap_route(quote_list: &[Rlp]) -> Result<SwapRoute, DecoderError> {
//    if quote_list.len() != 13 {
//        return Err(DecoderError::RlpIncorrectListLen);
//    }
//    let route = quote_list[0]
//        .iter()
//        .map(decode_pool_hop)
//        .collect::<Result<Vec<_>, _>>()?;
//    let path = quote_list[1]
//        .iter()
//        .map(|r| r.as_val())
//        .collect::<Result<Vec<_>, _>>()?;
//    let protocol = quote_list[2].as_val()?;
//    let amount_in = quote_list[3].as_val()?;
//    let amount_out = quote_list[4].as_val()?;
//    let execution_price = quote_list[5].as_val().ok().filter(|s| !s.is_empty());
//    let price_impact = quote_list[6].as_val().ok().filter(|s| !s.is_empty());
//    let score = quote_list[7]
//        .as_val::<String>()?
//        .parse::<u32>()
//        .map_err(|_| DecoderError::Custom("Invalid score"))?;
//    let trade_type = quote_list[8].as_val().ok().filter(|s| !s.is_empty());
//    let gas_limit = quote_list[9].as_val().ok().filter(|s| !s.is_empty());
//    let gas_limit_unit = quote_list[10].as_val().ok().filter(|s| !s.is_empty());
//    let max_gas_fee = quote_list[11].as_val().ok().filter(|s| !s.is_empty());
//    let max_gas_fee_unit = quote_list[12].as_val().ok().filter(|s| !s.is_empty());
//    Ok(SwapRoute {
//        route,
//        path,
//        protocol,
//        amount_in,
//        amount_out,
//        execution_price,
//        price_impact,
//        score,
//        trade_type,
//        gas_limit,
//        gas_limit_unit,
//        max_gas_fee,
//        max_gas_fee_unit,
//    })
//}
//
//fn decode_icp_quote(quote_list: &[Rlp]) -> Result<ICPQuoteResponse, DecoderError> {
//    if quote_list.len() != 10 {
//        return Err(DecoderError::RlpIncorrectListLen);
//    }
//    let token_in = quote_list[0].as_val()?;
//    let token_out = quote_list[1].as_val()?;
//    let amount_in = quote_list[2].as_val()?;
//    let amount_out = quote_list[3].as_val()?;
//    let execution_price = quote_list[4].as_val().ok().filter(|s| !s.is_empty());
//    let price_impact = quote_list[5].as_val().ok().filter(|s| !s.is_empty());
//    let route_string = quote_list[6].as_val().ok().filter(|s| !s.is_empty());
//    let route = quote_list[7]
//        .iter()
//        .map(decode_pool_hop)
//        .collect::<Result<Vec<_>, _>>()?;
//    let path = quote_list[8]
//        .iter()
//        .map(|r| r.as_val())
//        .collect::<Result<Vec<_>, _>>()?;
//    let score = quote_list[9]
//        .as_val::<String>()?
//        .parse::<u32>()
//        .map_err(|_| DecoderError::Custom("Invalid score"))?;
//    Ok(ICPQuoteResponse {
//        token_in,
//        token_out,
//        amount_in,
//        amount_out,
//        execution_price,
//        price_impact,
//        route_string,
//        route,
//        path,
//        score,
//    })
//}
//
//fn convert_to_quote_data(rlp_data: RlpQuoteData) -> Result<CrossChainQuoteData, DecoderError> {
//    let steps = rlp_data
//        .steps
//        .into_iter()
//        .map(|step| {
//            let quote = if step.chain_id == "icp" {
//                Quote::ICPQuoteResponse(decode_icp_quote(&step.quote)?)
//            } else {
//                Quote::SwapRoute(decode_swap_route(&step.quote)?)
//            };
//            Ok(CrossChainStep {
//                step: step
//                    .step
//                    .parse::<u32>()
//                    .map_err(|_| DecoderError::Custom("Invalid step"))?,
//                chain: step.chain,
//                chain_id: step.chain_id,
//                quote,
//            })
//        })
//        .collect::<Result<Vec<_>, DecoderError>>()?;
//    Ok(CrossChainQuoteData {
//        total_amount_in: rlp_data.total_amount_in,
//        total_amount_out: rlp_data.total_amount_out,
//        steps,
//    })
//}
//
//fn deserialize_cross_chain_quote(hex_str: &str) -> Result<CrossChainQuoteData, DecodeError> {
//    if !hex_str.starts_with("0x") {
//        return Err(DecodeError::MissingPrefix);
//    }
//    let bytes = hex::decode(&hex_str[2..])?;
//    let rlp_data: RlpQuoteData = rlp::decode(&bytes)?;
//    convert_to_quote_data(rlp_data).map_err(Into::into)
//}
//
//// Example usage (uncomment to test):
////
//fn main() -> Result<(), Box<dyn std::error::Error>> {
//    let hex_str = "0x..."; // Replace with actual encoded data
//    let quote_data = deserialize_cross_chain_quote(hex_str)?;
//    println!("{:?}", quote_data);
//    Ok(())
//}
//*/
