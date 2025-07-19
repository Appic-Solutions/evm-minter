use libsecp256k1::PublicKey;
use std::fmt;
use std::fmt::{Display, Formatter};
use std::str::FromStr;

use crate::eth_types::Address;

#[cfg(test)]
mod tests;

pub fn ecdsa_public_key_to_address(pubkey: &PublicKey) -> Address {
    let key_bytes = pubkey.serialize();
    debug_assert_eq!(key_bytes[0], 0x04);
    let hash = keccak(&key_bytes[1..]);
    let mut addr = [0u8; 20];
    addr[..].copy_from_slice(&hash[12..32]);
    Address::new(addr)
}

fn keccak(bytes: &[u8]) -> [u8; 32] {
    ic_sha3::Keccak256::hash(bytes)
}

#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum AddressValidationError {
    Invalid { error: String },
    NotSupported(Address),
}

impl Display for AddressValidationError {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        match self {
            AddressValidationError::Invalid { error } => {
                write!(f, "Invalid address: {}", error)
            }
            AddressValidationError::NotSupported(address) => {
                write!(f, "Address {} is not supported", address)
            }
        }
    }
}

/// Validate whether the given address can be used as the destination of an Ethereum transaction.
pub fn validate_address_as_destination(address: &str) -> Result<Address, AddressValidationError> {
    let address =
        Address::from_str(address).map_err(|e| AddressValidationError::Invalid { error: e })?;
    if address == Address::ZERO {
        return Err(AddressValidationError::NotSupported(address));
    }

    Ok(address)
}
