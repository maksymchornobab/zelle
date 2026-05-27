use serde::{Serialize, Deserialize};
use blake3::Hasher;
use ed25519_dalek::{VerifyingKey, Signature, Verifier};

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub struct Transaction {
    pub sender_address: String,
    pub ephemeral_receiver: String, // Одноразова криптографічна точка (One-Time Address)
    pub amount: f64,
    pub ring_public_keys: Vec<String>, // Для кільцевої стелс-анонімності
    pub tx_id: String,
    pub signature: Vec<u8>,
}

impl Transaction {
    pub fn new(sender: String, ephemeral_receiver: String, amount: f64, ring_keys: Vec<String>) -> Self {
        let mut tx = Transaction {
            sender_address: sender,
            ephemeral_receiver,
            amount,
            ring_public_keys: ring_keys,
            tx_id: String::new(),
            signature: Vec::new(),
        };
        tx.tx_id = tx.calculate_hash();
        tx
    }

    pub fn calculate_hash(&self) -> String {
        let mut hasher = Hasher::new();
        hasher.update(self.sender_address.as_bytes());
        hasher.update(self.ephemeral_receiver.as_bytes());
        hasher.update(&self.amount.to_be_bytes());
        for key in &self.ring_public_keys {
            hasher.update(key.as_bytes());
        }
        hasher.finalize().to_hex().to_string()
    }

    pub fn verify_signature(&self) -> bool {
        if self.sender_address == "SYSTEM" { return true; }
        if self.signature.is_empty() { return false; }

        let signature_array: [u8; 64] = match self.signature.clone().try_into() {
            Ok(arr) => arr,
            Err(_) => return false,
        };
        let signature = Signature::from_bytes(&signature_array);

        for pub_key_hex in &self.ring_public_keys {
            if let Ok(pub_key_bytes) = hex::decode(pub_key_hex) {
                if let Ok(pub_key_array) = pub_key_bytes.try_into() {
                    if let Ok(verifying_key) = VerifyingKey::from_bytes(&pub_key_array) {
                        if verifying_key.verify(self.tx_id.as_bytes(), &signature).is_ok() {
                            return true; 
                        }
                    }
                }
            }
        }
        false
    }
}