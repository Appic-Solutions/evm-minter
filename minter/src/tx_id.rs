use candid::Nat;
use minicbor::{Decode, Encode};

#[derive(Encode, Decode, Clone, PartialEq, Eq, PartialOrd, Ord, Debug)]
pub struct SwapTxId(#[n(0)] pub String);

impl SwapTxId {
    pub fn new(from_chain_id: &str, ledger_index: Nat, timestamp_ns: u64) -> Self {
        let timestamp_ms = timestamp_ns / 1_000_000;
        Self(format!("{from_chain_id}-{ledger_index}-{timestamp_ms}"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_new_basic() {
        let id = SwapTxId::new("1", Nat::from(2_u8), 3_000_000);
        assert_eq!(id.0, "1-2-3");
    }

    #[test]
    fn test_new_with_flooring() {
        let id = SwapTxId::new("42", Nat::from(100_u8), 1_500_000);
        assert_eq!(id.0, "42-100-1");
    }

    #[test]
    fn test_new_zeros() {
        let id = SwapTxId::new("0", Nat::from(0_u8), 0);
        assert_eq!(id.0, "0-0-0");
    }

    #[test]
    fn test_new_large_values() {
        let id = SwapTxId::new("5", Nat::from(u64::MAX), u64::MAX);
        assert_eq!(id.0, format!("{}-{}-{}", 5, u64::MAX, u64::MAX / 1_000_000));
    }
}
