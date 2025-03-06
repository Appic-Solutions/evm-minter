use ethnum::u256;
use ic_canister_log::log;
use minicbor;
use minicbor::{Decode, Encode};
use rlp::RlpStream;
use secp256k1::ecdsa::{RecoverableSignature, RecoveryId};

use crate::guard::TimerGuard;
use crate::logs::{DEBUG, INFO};
use crate::rpc_client::{MultiCallError, RpcClient};
use crate::rpc_declarations::{
    BlockSpec, BlockTag, FeeHistory, FeeHistoryParams, Hash, Quantity, TransactionStatus,
};
use crate::state::{lazy_call_ecdsa_public_key, TaskType};
use crate::state::{mutate_state, read_state};
use crate::{
    eth_types::Address,
    numeric::{BlockNumber, GasAmount, TransactionNonce, Wei, WeiPerGas},
    rpc_declarations::TransactionReceipt,
};
use ic_management_canister_types::DerivationPath;

use secp256k1::{ecdsa::Signature, Message, PublicKey, Secp256k1};

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
        let signature =
            crate::management::sign_with_ecdsa(key_name, DerivationPath::new(vec![]), hash.0)
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

    let secp = Secp256k1::verification_only();
    let msg = Message::from_digest(digest.0);
    let sig = Signature::from_compact(signature)
        .expect("compact signatures are 64 bytes; DER signatures are 68-72 bytes");

    // Ensure that the signature verification passes.
    debug_assert!(
        secp.verify_ecdsa(&msg, &sig, &ecdsa_public_key).is_ok(),
        "failed to verify signature prehashed, digest: {:?}, signature: {:?}, public_key: {:?}",
        hex::encode(digest.0),
        hex::encode(signature),
        hex::encode(ecdsa_public_key.serialize_uncompressed()),
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
    let secp = Secp256k1::new();

    // Try both possible recovery IDs: 0 (even y) and 1 (odd y)
    for rec in 0..=1 {
        let recovery_id = RecoveryId::try_from(rec).ok()?;
        if let Ok(recoverable_sig) = RecoverableSignature::from_compact(sig, recovery_id) {
            let message = Message::from_digest(digest.0);
            if let Ok(recovered_pubkey) = secp.recover_ecdsa(&message, &recoverable_sig) {
                if &recovered_pubkey == public_key {
                    return Some(rec == 1);
                }
            }
        }
    }
    None
}

/// Represents an estimate of gas fees.
///
/// Contains the base fee per gas and the maximum priority fee per gas.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct GasFeeEstimate {
    pub base_fee_per_gas: WeiPerGas,
    pub max_priority_fee_per_gas: WeiPerGas,
}

impl GasFeeEstimate {
    /// Computes the maximum fee per gas by doubling the base fee and adding the priority fee.
    ///
    /// # Returns
    /// An `Option` containing the estimated maximum fee per gas if it does not overflow, otherwise `None`.
    pub fn checked_estimate_max_fee_per_gas(&self) -> Option<WeiPerGas> {
        self.base_fee_per_gas
            .checked_mul(2_u8)
            .and_then(|base_fee_estimate| {
                base_fee_estimate.checked_add(self.max_priority_fee_per_gas)
            })
    }

    /// Estimates the maximum fee per gas. Falls back to `WeiPerGas::MAX` if the calculation fails.
    ///
    /// # Returns
    /// The estimated maximum fee per gas.
    pub fn estimate_max_fee_per_gas(&self) -> WeiPerGas {
        self.checked_estimate_max_fee_per_gas()
            .unwrap_or(WeiPerGas::MAX)
    }

    /// Converts the gas fee estimate to a transaction price with a specified gas limit.
    ///
    /// # Arguments
    /// * `gas_limit` - The gas limit for the transaction.
    ///
    /// # Returns
    /// A `TransactionPrice` containing the gas limit and fee estimates.
    pub fn to_price(self, gas_limit: GasAmount) -> TransactionPrice {
        TransactionPrice {
            gas_limit,
            max_fee_per_gas: self.estimate_max_fee_per_gas(),
            max_priority_fee_per_gas: self.max_priority_fee_per_gas,
        }
    }

    /// Computes the minimum of the maximum fee per gas by adding base and priority fees.
    /// Falls back to `WeiPerGas::MAX` if the calculation fails.
    ///
    /// # Returns
    /// The minimum maximum fee per gas.
    pub fn min_max_fee_per_gas(&self) -> WeiPerGas {
        self.base_fee_per_gas
            .checked_add(self.max_priority_fee_per_gas)
            .unwrap_or(WeiPerGas::MAX)
    }
}

/// Represents the price of a transaction.
///
/// Includes the gas limit, maximum fee per gas, and maximum priority fee per gas.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TransactionPrice {
    pub gas_limit: GasAmount,
    pub max_fee_per_gas: WeiPerGas,
    pub max_priority_fee_per_gas: WeiPerGas,
}

impl TransactionPrice {
    /// Computes the maximum transaction fee based on the gas limit and maximum fee per gas.
    ///
    /// # Returns
    /// The maximum transaction fee if calculation is successful, otherwise `Wei::MAX`.
    pub fn max_transaction_fee(&self) -> Wei {
        self.max_fee_per_gas
            .transaction_cost(self.gas_limit)
            .unwrap_or(Wei::MAX)
    }

    /// Estimates the new transaction price required to resubmit a transaction with updated gas fees.
    ///
    /// If the current transaction price is sufficient, it remains unchanged. Otherwise, it adjusts
    /// the maximum priority fee and possibly the maximum fee per gas to ensure the transaction can be resubmitted.
    ///
    /// # Arguments
    /// * `new_gas_fee` - The new gas fee estimate.
    ///
    /// # Returns
    /// A new `TransactionPrice` with updated values.
    pub fn resubmit_transaction_price(self, new_gas_fee: GasFeeEstimate) -> Self {
        let plus_10_percent = |amount: WeiPerGas| {
            amount
                .checked_add(
                    amount
                        .checked_div_ceil(10_u8)
                        .expect("BUG: must be Some() because divisor is non-zero"),
                )
                .unwrap_or(WeiPerGas::MAX)
        };

        if self.max_fee_per_gas >= new_gas_fee.min_max_fee_per_gas()
            && self.max_priority_fee_per_gas >= new_gas_fee.max_priority_fee_per_gas
        {
            self
        } else {
            // Increase max_priority_fee_per_gas by at least 10% if necessary, ensuring the transaction
            // remains resubmittable. Update max_fee_per_gas if it doesn't cover the new max_priority_fee_per_gas.
            let updated_max_priority_fee_per_gas = plus_10_percent(self.max_priority_fee_per_gas)
                .max(new_gas_fee.max_priority_fee_per_gas);
            let new_gas_fee = GasFeeEstimate {
                max_priority_fee_per_gas: updated_max_priority_fee_per_gas,
                ..new_gas_fee
            };
            let new_max_fee_per_gas = new_gas_fee.min_max_fee_per_gas().max(self.max_fee_per_gas);
            TransactionPrice {
                gas_limit: self.gas_limit,
                max_fee_per_gas: new_max_fee_per_gas,
                max_priority_fee_per_gas: updated_max_priority_fee_per_gas,
            }
        }
    }
}

/// Asynchronously refreshes the gas fee estimate.
///
/// Uses a cached estimate if it is recent enough. Otherwise, fetches the latest fee history and recalculates the estimate.
///
/// # Returns
/// An `Option` containing the new `GasFeeEstimate` if successful, or `None` if the refresh fails.
pub async fn lazy_refresh_gas_fee_estimate() -> Option<GasFeeEstimate> {
    const MAX_AGE_NS: u64 = 30_000_000_000_u64; // 30 seconds

    async fn do_refresh() -> Option<GasFeeEstimate> {
        let _guard = match TimerGuard::new(TaskType::RefreshGasFeeEstimate) {
            Ok(guard) => guard,
            Err(e) => {
                log!(
                    DEBUG,
                    "[refresh_gas_fee_estimate]: Failed retrieving guard: {e:?}",
                );
                return None;
            }
        };

        let mut attempts = 0;
        const MAX_ATTEMPTS: u32 = 3;

        let fee_history = loop {
            match get_fee_history().await {
                Ok(fee_history) => break fee_history, // Exit loop on success
                Err(e) => {
                    attempts += 1;
                    log!(
                        DEBUG,
                        "[refresh_gas_fee_estimate]: Failed retrieving fee history: {e:?} attempt {}/{} failed",
                        attempts,
                        MAX_ATTEMPTS
                    );

                    if attempts >= MAX_ATTEMPTS {
                        log!(
                        DEBUG,
                        "[refresh_gas_fee_estimate]: max retries reached. Skipping scrapping fee history fetching."
                    );
                        return None;
                    }
                }
            }
        };

        let gas_fee_estimate = match estimate_transaction_fee(&fee_history) {
            Ok(estimate) => {
                mutate_state(|s| {
                    s.last_transaction_price_estimate =
                        Some((ic_cdk::api::time(), estimate.clone()));
                });
                estimate
            }
            Err(e) => {
                log!(
                    INFO,
                    "[refresh_gas_fee_estimate]: Failed estimating gas fee: {e:?}",
                );
                return None;
            }
        };
        log!(
            INFO,
            "[refresh_gas_fee_estimate]: Estimated transaction fee: {:?}",
            gas_fee_estimate,
        );
        Some(gas_fee_estimate)
    }

    async fn get_fee_history() -> Result<FeeHistory, MultiCallError<FeeHistory>> {
        read_state(RpcClient::from_state_one_provider_public_node)
            .fee_history(FeeHistoryParams {
                block_count: Quantity::from(5_u8),
                highest_block: BlockSpec::Tag(BlockTag::Latest),
                reward_percentiles: vec![50],
            })
            .await
    }

    let now_ns = ic_cdk::api::time();
    match read_state(|s| s.last_transaction_price_estimate.clone()) {
        Some((last_estimate_timestamp_ns, estimate))
            if now_ns < last_estimate_timestamp_ns.saturating_add(MAX_AGE_NS) =>
        {
            Some(estimate)
        }
        _ => do_refresh().await,
    }
}

/// Possible errors when estimating transaction fees.
#[derive(Debug, PartialEq, Eq)]
pub enum TransactionFeeEstimationError {
    InvalidFeeHistory(String),
    Overflow(String),
}

/// Estimates

/// the transaction fee based on fee history.

/// Determines the base fee per gas for the next block and computes the maximum priority fee based on historic values.
/// Returns an estimate of the gas fee.
///
/// # Arguments
/// * `fee_history` - The fee history to use for estimation.
///
/// # Returns
/// A `Result` containing the `GasFeeEstimate` if successful, or an error if estimation fails.
pub fn estimate_transaction_fee(
    fee_history: &FeeHistory,
) -> Result<GasFeeEstimate, TransactionFeeEstimationError> {
    let min_max_priority_fee_per_gas: WeiPerGas =
        read_state(|state| state.min_max_priority_fee_per_gas); // Different on each network

    let base_fee_per_gas_next_block = *fee_history.base_fee_per_gas.last().ok_or(
        TransactionFeeEstimationError::InvalidFeeHistory(
            "base_fee_per_gas should not be empty to be able to evaluate transaction price"
                .to_string(),
        ),
    )?;

    let max_priority_fee_per_gas = {
        let mut rewards: Vec<&WeiPerGas> = fee_history.reward.iter().flatten().collect();
        let historic_max_priority_fee_per_gas =
            **median(&mut rewards).ok_or(TransactionFeeEstimationError::InvalidFeeHistory(
                "should be non-empty with rewards of the last 5 blocks".to_string(),
            ))?;
        historic_max_priority_fee_per_gas.max(min_max_priority_fee_per_gas)
    };

    let gas_fee_estimate = GasFeeEstimate {
        base_fee_per_gas: base_fee_per_gas_next_block,
        max_priority_fee_per_gas,
    };

    if gas_fee_estimate
        .checked_estimate_max_fee_per_gas()
        .is_none()
    {
        return Err(TransactionFeeEstimationError::Overflow(
            "max_fee_per_gas overflowed".to_string(),
        ));
    }

    Ok(gas_fee_estimate)
}

/// Computes the median of a slice of values.
///
/// # Arguments
/// * `values` - The slice of values to compute the median of.
///
/// # Returns
/// An `Option` containing the median value, or `None` if the slice is empty.
fn median<T: Ord>(values: &mut [T]) -> Option<&T> {
    if values.is_empty() {
        return None;
    }
    let (_, item, _) = values.select_nth_unstable(values.len() / 2);
    Some(item)
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
