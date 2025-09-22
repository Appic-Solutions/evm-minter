pub mod gas_fees;
pub mod gas_usd;

use ethnum::u256;
use gas_fees::{GasFeeEstimate, TransactionPrice};
use minicbor;
use minicbor::{Decode, Encode};
use rlp::RlpStream;

use crate::rpc_declarations::{Hash, TransactionStatus};
use crate::state::lazy_call_ecdsa_public_key;
use crate::state::read_state;
use crate::{
    eth_types::Address,
    numeric::{BlockNumber, GasAmount, TransactionNonce, Wei, WeiPerGas},
    rpc_declarations::TransactionReceipt,
};
//use ic_management_canister_types::DerivationPath;

use libsecp256k1::{recover, verify, Message, PublicKey, RecoveryId, Signature};

// Constant representing the transaction type identifier for EIP-1559 transactions.
const EIP1559_TX_ID: u8 = 2;

// The `AccessList` struct is a transparent wrapper around a vector of `AccessListItem`.
// It uses CBOR serialization and deserialization with a single field (hence transparent).
#[derive(Clone, Debug, Eq, Hash, PartialEq, Encode, Decode)]
#[cbor(transparent)]
pub struct AccessList(#[n(0)] pub Vec<AccessListItem>);

impl AccessList {
    // Creates a new, empty `AccessList`.
    pub fn new() -> Self {
        Self(Vec::new())
    }
}

// Provides a default implementation for `AccessList`,
// which simply returns an empty `AccessList`.
impl Default for AccessList {
    fn default() -> Self {
        Self::new()
    }
}

// Implements the RLP encoding trait for `AccessList`.
// This is needed to serialize `AccessList` into the RLP format,
// which is used in Ethereum for encoding transactions and other data structures.
impl rlp::Encodable for AccessList {
    fn rlp_append(&self, s: &mut RlpStream) {
        // Encodes the inner vector (`Vec<AccessListItem>`) using RLP.
        s.append_list(&self.0);
    }
}

// The `StorageKey` struct is a transparent wrapper around a 32-byte array.
// It uses CBOR serialization and deserialization with the `minicbor::bytes` option,
// which is used to handle the raw bytes in the CBOR encoding.
#[derive(Clone, Debug, Eq, Hash, PartialEq, Encode, Decode)]
#[cbor(transparent)]
pub struct StorageKey(#[cbor(n(0), with = "minicbor::bytes")] pub [u8; 32]);

// The `AccessListItem` struct represents an individual item in the access list.
// Each item contains an Ethereum address and a list of storage keys that are accessed.
#[derive(Clone, Debug, Eq, Hash, PartialEq, Encode, Decode)]
pub struct AccessListItem {
    /// The Ethereum address being accessed.
    #[n(0)]
    pub address: Address,
    /// The storage keys accessed by the address.
    #[n(1)]
    pub storage_keys: Vec<StorageKey>,
}

// Implements the RLP encoding trait for `AccessListItem`.
// This is necessary to encode each `AccessListItem` into RLP format,
// as part of the overall `AccessList` RLP encoding.
impl rlp::Encodable for AccessListItem {
    fn rlp_append(&self, s: &mut RlpStream) {
        const ACCESS_FIELD_COUNT: usize = 2; // There are two fields: address and storage_keys.

        s.begin_list(ACCESS_FIELD_COUNT); // Begin the RLP list for the `AccessListItem`.
        s.append(&self.address.as_ref()); // Encode the address as a byte array.

        // Encode the list of storage keys.
        s.begin_list(self.storage_keys.len());
        for storage_key in self.storage_keys.iter() {
            s.append(&storage_key.0.as_ref()); // Encode each storage key as a byte array.
        }
    }
}

/// Struct representing an EIP-1559 transaction request.
/// EIP-1559 introduced a new transaction format for Ethereum with a more dynamic fee structure.
/// Documentation: <https://eips.ethereum.org/EIPS/eip-1559>
#[derive(Clone, Debug, Eq, PartialEq, Encode, Decode)]
pub struct Eip1559TransactionRequest {
    #[n(0)]
    pub chain_id: u64, // Chain ID to identify the network (e.g., Ethereum mainnet, testnets).
    #[n(1)]
    pub nonce: TransactionNonce, // Transaction nonce to ensure each transaction is unique.
    #[n(2)]
    pub max_priority_fee_per_gas: WeiPerGas, // Maximum tip the sender is willing to pay to miners.
    #[n(3)]
    pub max_fee_per_gas: WeiPerGas, // Maximum total fee (base fee + priority fee) the sender is willing to pay.
    #[n(4)]
    pub gas_limit: GasAmount, // Maximum amount of gas that can be used by the transaction.
    #[n(5)]
    pub destination: Address, // Address to which the transaction is sent.
    #[n(6)]
    pub amount: Wei, // Amount of Ether to be transferred in the transaction.
    #[cbor(n(7), with = "minicbor::bytes")]
    pub data: Vec<u8>, // Optional data payload for contract interaction or additional instructions.
    #[n(8)]
    pub access_list: AccessList, // Access list for the transaction, which is a list of addresses and storage keys.
}

// Implements the `AsRef` trait for `Eip1559TransactionRequest` to return a reference to itself.
impl AsRef<Eip1559TransactionRequest> for Eip1559TransactionRequest {
    fn as_ref(&self) -> &Eip1559TransactionRequest {
        self
    }
}

/// Generic struct that wraps a transaction and its associated resubmission strategy.
/// This is used for managing transactions that may need to be resubmitted due to network conditions.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Resubmittable<T> {
    pub transaction: T,                     // The transaction being wrapped.
    pub resubmission: ResubmissionStrategy, // Strategy to use when resubmitting the transaction.
}

// Type alias for a resubmittable EIP-1559 transaction request.
pub type TransactionRequest = Resubmittable<Eip1559TransactionRequest>;

// Type alias for a resubmittable signed EIP-1559 transaction request.
pub type SignedTransactionRequest = Resubmittable<SignedEip1559TransactionRequest>;

// Implements a method to clone the resubmission strategy and apply it to a different transaction.
impl<T> Resubmittable<T> {
    pub fn clone_resubmission_strategy<V>(&self, other: V) -> Resubmittable<V> {
        Resubmittable {
            transaction: other,                      // The new transaction to be wrapped.
            resubmission: self.resubmission.clone(), // Cloned resubmission strategy from the original transaction.
        }
    }
}

// Implements the `AsRef` trait to return a reference to the wrapped transaction.
impl<T> AsRef<T> for Resubmittable<T> {
    fn as_ref(&self) -> &T {
        &self.transaction
    }
}

/// Enum representing different strategies for resubmitting a transaction.
/// These strategies determine how to adjust the transaction parameters, such as fees, during resubmission.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ResubmissionStrategy {
    ReduceEthAmount { withdrawal_amount: Wei }, // Strategy to reduce the Ether amount sent to cover fees.
    GuaranteeEthAmount { allowed_max_transaction_fee: Wei }, // Strategy to ensure a specific amount of Ether is sent, regardless of fees.
}

// Implements methods for `ResubmissionStrategy` to retrieve the maximum allowed transaction fee based on the strategy.
impl ResubmissionStrategy {
    pub fn allowed_max_transaction_fee(&self) -> Wei {
        match self {
            ResubmissionStrategy::ReduceEthAmount { withdrawal_amount } => *withdrawal_amount,
            ResubmissionStrategy::GuaranteeEthAmount {
                allowed_max_transaction_fee,
            } => *allowed_max_transaction_fee,
        }
    }
}

/// Enum representing potential errors that can occur during transaction resubmission.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ResubmitTransactionError {
    InsufficientTransactionFee {
        allowed_max_transaction_fee: Wei, // The maximum fee allowed by the resubmission strategy.
        actual_max_transaction_fee: Wei,  // The actual fee required by the new transaction.
    },
}

// Implements a method for resubmitting a signed transaction request with a new gas fee estimate.
impl SignedTransactionRequest {
    pub fn resubmit(
        &self,
        new_gas_fee: GasFeeEstimate,
    ) -> Result<Option<Eip1559TransactionRequest>, ResubmitTransactionError> {
        // Retrieve the current transaction request.
        let transaction_request = self.transaction.transaction();
        // Get the current transaction price (gas price).
        let last_tx_price = transaction_request.transaction_price();
        // Calculate the new transaction price with the updated gas fee.
        let new_tx_price = last_tx_price
            .clone()
            .resubmit_transaction_price(new_gas_fee);
        // If the new price is the same as the old one, no need to resubmit.
        if new_tx_price == last_tx_price {
            return Ok(None);
        }

        // Check if the new transaction fee exceeds the allowed maximum fee.
        if new_tx_price.max_transaction_fee() > self.resubmission.allowed_max_transaction_fee() {
            return Err(ResubmitTransactionError::InsufficientTransactionFee {
                allowed_max_transaction_fee: self.resubmission.allowed_max_transaction_fee(),
                actual_max_transaction_fee: new_tx_price.max_transaction_fee(),
            });
        }

        // Calculate the new amount to send, adjusting for the transaction fee.
        let new_amount = match self.resubmission {
            ResubmissionStrategy::ReduceEthAmount { withdrawal_amount } => {
                withdrawal_amount.checked_sub(new_tx_price.max_transaction_fee())
                    .expect("BUG: withdrawal_amount covers new transaction fee because it was checked before")
            }
            ResubmissionStrategy::GuaranteeEthAmount { .. } => transaction_request.amount,
        };

        // Return the new transaction request with updated parameters.
        Ok(Some(Eip1559TransactionRequest {
            max_priority_fee_per_gas: new_tx_price.max_priority_fee_per_gas,
            max_fee_per_gas: new_tx_price.max_fee_per_gas,
            gas_limit: new_tx_price.gas_limit,
            amount: new_amount,
            ..transaction_request.clone()
        }))
    }
}

// Implements RLP encoding for the `Eip1559TransactionRequest` struct.
// This allows the transaction request to be serialized into RLP format, which is required for Ethereum transactions.
impl rlp::Encodable for Eip1559TransactionRequest {
    fn rlp_append(&self, s: &mut RlpStream) {
        s.begin_unbounded_list(); // Begin the RLP encoding as an unbounded list.
        self.rlp_inner(s); // Encode the inner fields of the transaction request.
        s.finalize_unbounded_list(); // Finalize the RLP encoding.
    }
}

// Deriving traits for Eip1559Signature struct. Default creates a default instance,
// Clone allows cloning the struct, PartialEq and Eq enable comparisons, Hash allows hashing,
// Debug provides a way to print the struct, Encode and Decode are for serialization and deserialization.
#[derive(Default, Clone, PartialEq, Eq, Hash, Debug, Encode, Decode)]
pub struct Eip1559Signature {
    // n(0) indicates this field is encoded at position 0 in some serialization formats.
    #[n(0)]
    pub signature_y_parity: bool, // Boolean value representing the parity of the signature's y coordinate.

    // r and s are components of the ECDSA signature, stored as u256 (256-bit unsigned integers).
    // cbor(n) indicates CBOR serialization with custom logic provided in "crate::cbor::u256".
    #[cbor(n(1), with = "crate::cbor::u256")]
    pub r: u256, // r component of the ECDSA signature.

    #[cbor(n(2), with = "crate::cbor::u256")]
    pub s: u256, // s component of the ECDSA signature.
}

// Implementing rlp::Encodable for Eip1559Signature to support RLP encoding.
// RLP (Recursive Length Prefix) is used for encoding in Ethereum.
impl rlp::Encodable for Eip1559Signature {
    fn rlp_append(&self, s: &mut RlpStream) {
        s.append(&self.signature_y_parity); // Append y_parity to the RLP stream.
        encode_u256(s, self.r); // Append r to the RLP stream, using custom encoding.
        encode_u256(s, self.s); // Append s to the RLP stream, using custom encoding.
    }
}

// Represents an immutable, signed EIP-1559 transaction.
// The transaction is signed, so it can't be modified after creation.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SignedEip1559TransactionRequest {
    inner: InnerSignedTransactionRequest, // Inner struct containing the actual transaction and signature.

    // Hash of the signed transaction. It's computed once and memoized for efficiency.
    // The hash is used to identify the transaction uniquely.
    memoized_hash: Hash,
}

// Implementing AsRef to allow easy access to the underlying Eip1559TransactionRequest.
impl AsRef<Eip1559TransactionRequest> for SignedEip1559TransactionRequest {
    fn as_ref(&self) -> &Eip1559TransactionRequest {
        &self.inner.transaction // Returns a reference to the transaction inside the inner struct.
    }
}

// Inner struct representing the transaction and its signature.
#[derive(Clone, Debug, Eq, PartialEq, Encode, Decode)]
struct InnerSignedTransactionRequest {
    #[n(0)]
    transaction: Eip1559TransactionRequest, // The actual EIP-1559 transaction.

    #[n(1)]
    signature: Eip1559Signature, // The signature for the transaction.
}

// Implementing RLP encoding for InnerSignedTransactionRequest.
impl rlp::Encodable for InnerSignedTransactionRequest {
    fn rlp_append(&self, s: &mut RlpStream) {
        s.begin_unbounded_list(); // Start an unbounded list in the RLP stream.
        self.transaction.rlp_inner(s); // Append the inner transaction data.
        s.append(&self.signature); // Append the signature data.
        s.finalize_unbounded_list(); // Finalize the unbounded list.
    }
}

// Methods related to InnerSignedTransactionRequest.
impl InnerSignedTransactionRequest {
    // Returns the raw bytes of the signed transaction in EIP-1559 format.
    // This includes a transaction type byte (0x02) followed by the RLP encoding.
    pub fn raw_bytes(&self) -> Vec<u8> {
        use rlp::Encodable;
        let mut rlp = self.rlp_bytes().to_vec(); // Convert RLP-encoded transaction to bytes.
        rlp.insert(0, self.transaction.transaction_type()); // Prepend the transaction type (0x02).
        rlp
    }
}

// Implementing CBOR encoding for SignedEip1559TransactionRequest using minicbor.
impl<C> minicbor::Encode<C> for SignedEip1559TransactionRequest {
    fn encode<W: minicbor::encode::Write>(
        &self,
        e: &mut minicbor::Encoder<W>,
        ctx: &mut C,
    ) -> Result<(), minicbor::encode::Error<W::Error>> {
        e.encode_with(&self.inner, ctx)?; // Encode the inner transaction request.
        Ok(())
    }
}

// Implementing CBOR decoding for SignedEip1559TransactionRequest using minicbor.
impl<'b, C> minicbor::Decode<'b, C> for SignedEip1559TransactionRequest {
    fn decode(d: &mut minicbor::Decoder<'b>, ctx: &mut C) -> Result<Self, minicbor::decode::Error> {
        d.decode_with(ctx)
            .map(|inner: InnerSignedTransactionRequest| {
                Self::new(inner.transaction, inner.signature) // Create a new instance from the decoded data.
            })
    }
}

// FinalizedEip1559Transaction represents an immutable finalized transaction, which includes
// the signed transaction and a receipt.
#[derive(Clone, Debug, Eq, PartialEq, Encode, Decode)]
pub struct FinalizedEip1559Transaction {
    #[n(0)]
    transaction: SignedEip1559TransactionRequest, // The signed transaction.

    #[n(1)]
    receipt: TransactionReceipt, // The transaction receipt, which includes details like block number and status.
}

// Implementing AsRef to allow easy access to the underlying Eip1559TransactionRequest.
impl AsRef<Eip1559TransactionRequest> for FinalizedEip1559Transaction {
    fn as_ref(&self) -> &Eip1559TransactionRequest {
        self.transaction.as_ref() // Returns a reference to the transaction inside the signed transaction.
    }
}

// Various methods to access properties of the finalized transaction.
impl FinalizedEip1559Transaction {
    // Returns the destination address of the transaction.
    pub fn destination(&self) -> &Address {
        &self.transaction.transaction().destination
    }

    // Returns the block number where the transaction was included.
    pub fn block_number(&self) -> &BlockNumber {
        &self.receipt.block_number
    }

    // Returns the amount transferred in the transaction.
    pub fn transaction_amount(&self) -> &Wei {
        &self.transaction.transaction().amount
    }

    // Returns the hash of the transaction.
    pub fn transaction_hash(&self) -> &Hash {
        &self.receipt.transaction_hash
    }

    // Returns the data field of the transaction.
    pub fn transaction_data(&self) -> &[u8] {
        &self.transaction.transaction().data
    }

    // Returns the EIP-1559 transaction request.
    pub fn transaction(&self) -> &Eip1559TransactionRequest {
        self.transaction.transaction()
    }

    // Returns the transaction price, including gas limit and fees.
    pub fn transaction_price(&self) -> TransactionPrice {
        self.transaction.transaction().transaction_price()
    }

    // Calculates and returns the effective transaction fee based on the gas used and price.
    pub fn effective_transaction_fee(&self) -> Wei {
        self.receipt.effective_transaction_fee()
    }

    // Returns the status of the transaction (e.g., success or failure).
    pub fn transaction_status(&self) -> &TransactionStatus {
        &self.receipt.status
    }
}

// Implementing conversion from a tuple of Eip1559TransactionRequest and Eip1559Signature
// to SignedEip1559TransactionRequest.
impl From<(Eip1559TransactionRequest, Eip1559Signature)> for SignedEip1559TransactionRequest {
    fn from((transaction, signature): (Eip1559TransactionRequest, Eip1559Signature)) -> Self {
        Self::new(transaction, signature) // Create a new signed transaction request.
    }
}

// Implementing RLP encoding for SignedEip1559TransactionRequest.
impl rlp::Encodable for SignedEip1559TransactionRequest {
    fn rlp_append(&self, s: &mut RlpStream) {
        s.append(&self.inner); // Append the inner signed transaction request to the RLP stream.
    }
}

// Methods related to SignedEip1559TransactionRequest.
impl SignedEip1559TransactionRequest {
    // Creates a new signed transaction request and computes its hash.
    pub fn new(transaction: Eip1559TransactionRequest, signature: Eip1559Signature) -> Self {
        let inner = InnerSignedTransactionRequest {
            transaction,
            signature,
        };
        let hash = Hash(ic_sha3::Keccak256::hash(inner.raw_bytes())); // Compute the hash.
        Self {
            inner,
            memoized_hash: hash, // Store the computed hash.
        }
    }

    // Returns the transaction in raw hex format, including the transaction type prefix.
    pub fn raw_transaction_hex(&self) -> String {
        format!("0x{}", hex::encode(self.inner.raw_bytes()))
    }

    // Returns the hash of the signed transaction.
    pub fn hash(&self) -> Hash {
        self.memoized_hash
    }

    // Returns a reference to the underlying transaction request.
    pub fn transaction(&self) -> &Eip1559TransactionRequest {
        &self.inner.transaction
    }

    // Returns the nonce of the transaction.
    pub fn nonce(&self) -> TransactionNonce {
        self.transaction().nonce
    }

    // Attempts to finalize the transaction with a receipt.
    // Checks that the hash, gas price, and gas limit match between the transaction and receipt.
    pub fn try_finalize(
        self,
        receipt: TransactionReceipt,
    ) -> Result<FinalizedEip1559Transaction, String> {
        if self.hash() != receipt.transaction_hash {
            return Err(format!(
                "transaction hash mismatch: expected {}, got {}",
                self.hash(),
                receipt.transaction_hash
            ));
        }
        if self.transaction().max_fee_per_gas < receipt.effective_gas_price {
            return Err(format!(
                "transaction max_fee_per_gas {} is smaller than effective_gas_price {}",
                self.transaction().max_fee_per_gas,
                receipt.effective_gas_price
            ));
        }
        if self.transaction().gas_limit < receipt.gas_used {
            return Err(format!(
                "transaction gas limit {} is smaller than gas used {}",
                self.transaction().gas_limit,
                receipt.gas_used
            ));
        }
        Ok(FinalizedEip1559Transaction {
            transaction: self,
            receipt,
        })
    }
}

// Helper function to encode a u256 value into an RLP stream.
pub fn encode_u256<T: Into<u256>>(stream: &mut RlpStream, value: T) {
    let value = value.into();
    let leading_empty_bytes: usize = value.leading_zeros() as usize / 8; // Calculate leading zeros.
    stream.append(&value.to_be_bytes()[leading_empty_bytes..].as_ref()); // Append the non-zero part.
}

// Methods related to Eip1559TransactionRequest.
impl Eip1559TransactionRequest {
    // Returns the transaction type identifier (0x02 for EIP-1559).
    pub fn transaction_type(&self) -> u8 {
        EIP1559_TX_ID
    }

    // Encodes the inner fields of the transaction using RLP.
    pub fn rlp_inner(&self, rlp: &mut RlpStream) {
        rlp.append(&self.chain_id);
        rlp.append(&self.nonce);
        rlp.append(&self.max_priority_fee_per_gas);
        rlp.append(&self.max_fee_per_gas);
        rlp.append(&self.gas_limit);
        rlp.append(&self.destination.as_ref());
        rlp.append(&self.amount);
        rlp.append(&self.data);
        rlp.append(&self.access_list);
    }

    // Computes and returns the hash of the transaction.
    pub fn hash(&self) -> Hash {
        use rlp::Encodable;
        let mut bytes = self.rlp_bytes().to_vec();
        bytes.insert(0, self.transaction_type());
        Hash(ic_sha3::Keccak256::hash(bytes))
    }

    // Returns the transaction price, including gas limit and fees.
    pub fn transaction_price(&self) -> TransactionPrice {
        TransactionPrice {
            gas_limit: self.gas_limit,
            max_fee_per_gas: self.max_fee_per_gas,
            max_priority_fee_per_gas: self.max_priority_fee_per_gas,
        }
    }

    // Asynchronously signs the transaction using the ECDSA key and returns a signed transaction request.
    pub async fn sign(self) -> Result<SignedEip1559TransactionRequest, String> {
        let hash = self.hash(); // Compute the transaction hash.
        let key_name = read_state(|s| s.ecdsa_key_name.clone()); // Retrieve the ECDSA key name.
        let signature = crate::management::sign_with_ecdsa(key_name, vec![], hash.0)
            .await
            .map_err(|e| format!("failed to sign tx: {}", e))?; // Sign the hash with the ECDSA key.

        let public_key = verifiy_signature(&hash, &signature).await; // Compute the recovery ID.
        let signature_y_parity = determine_signature_y_parity(&public_key, &hash, &signature)
            .expect("Bug: Failed to determine y parity");
        let (r_bytes, s_bytes) = split_in_two(signature); // Split the signature into r and s components.
        let r = u256::from_be_bytes(r_bytes);
        let s = u256::from_be_bytes(s_bytes);

        let sig = Eip1559Signature {
            signature_y_parity,
            r,
            s,
        };

        Ok(SignedEip1559TransactionRequest::new(self, sig)) // Return the signed transaction request.
    }
}

/// Computes the recovery ID from a given digest and signature.
///
/// This function asynchronously fetches the ECDSA public key, verifies the provided signature against the digest,
/// and then attempts to recover the public key from the digest and signature. If the recovery fails, it panics.
///
/// # Arguments
/// * `digest` - The hash digest of the message to be verified.
/// * `signature` - The signature to verify against the digest.
///
/// # Returns
/// The recovered public key if successful.
///
/// # Panics
/// Panics if the signature verification or public key recovery fails.
async fn verifiy_signature(digest: &Hash, signature: &[u8]) -> PublicKey {
    let ecdsa_public_key = lazy_call_ecdsa_public_key().await;

    let msg = Message::parse(&digest.0);
    let sig = Signature::parse_standard_slice(signature)
        .expect("compact signatures are 64 bytes; DER signatures are 68-72 bytes");

    // Ensure that the signature verification passes.
    debug_assert!(
        verify(&msg, &sig, &ecdsa_public_key),
        "failed to verify signature prehashed, digest: {:?}, signature: {:?}, public_key: {:?}",
        hex::encode(digest.0),
        hex::encode(signature),
        hex::encode(ecdsa_public_key.serialize()),
    );

    ecdsa_public_key
}

/// Determines the signature_y_parity (i.e. the recovery bit) from a signature given the known public key,
/// a 32-byte message hash, and a 64-byte signature (r and s concatenated).
///
/// Returns:
/// - Some(true) if the recovery id is 1 (odd y coordinate),
/// - Some(false) if the recovery id is 0 (even y coordinate),
/// - None if neither candidate recovers the known public key.
pub fn determine_signature_y_parity(
    public_key: &PublicKey,
    digest: &Hash,
    sig: &[u8; 64],
) -> Option<bool> {
    let msg = Message::parse(&digest.0);
    let sig = Signature::parse_standard_slice(sig)
        .expect("compact signatures are 64 bytes; DER signatures are 68-72 bytes");

    // Try both possible recovery IDs: 0 (even y) and 1 (odd y)
    for rec in 0..=1 {
        let recovery_id = RecoveryId::parse(rec).ok()?;
        if let Ok(recovered_pk) = recover(&msg, &sig, &recovery_id) {
            if recovered_pk == *public_key {
                return Some(rec == 1);
            }
        }
    }
    None
}

/// Splits an array into two halves.
///
/// # Arguments
/// * `array` - The array to be split.
///
/// # Returns
/// A tuple containing the two halves of the array.
fn split_in_two(array: [u8; 64]) -> ([u8; 32], [u8; 32]) {
    let mut r = [0u8; 32];
    let mut s = [0u8; 32];
    r.copy_from_slice(&array[..32]);
    s.copy_from_slice(&array[32..]);
    (r, s)
}
