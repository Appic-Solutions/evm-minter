use crate::rpc_declarations::FeeHistory;

pub fn parse_fee_history(fee_history: String) -> Option<FeeHistory> {
    let fee_history_parsed = serde_json::from_str::<FeeHistory>(&fee_history).ok()?;

    Some(fee_history_parsed)
}
