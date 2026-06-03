use serde::{Serialize, Deserialize};
use blake3::Hasher;
use crate::wallet::RingSignature; 

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct Transaction {
    #[serde(skip_serializing, default)] 
    pub sender_address: String,
    
    pub ephemeral_receiver: String, 
    pub amount: f64,
    pub ring_public_keys: Vec<String>, 
    pub tx_id: String,
    pub signature: Option<RingSignature>,
}

impl Transaction {
    pub fn new(sender: String, ephemeral_receiver: String, amount: f64, ring_keys: Vec<String>) -> Self {
        let mut tx = Transaction {
            sender_address: sender,
            ephemeral_receiver,
            amount,
            ring_public_keys: ring_keys,
            tx_id: String::new(),
            signature: None, // За замовчуванням підпису немає
        };
        tx.tx_id = tx.calculate_hash();
        tx
    }

    pub fn calculate_hash(&self) -> String {
        let mut hasher = Hasher::new();
        hasher.update(self.ephemeral_receiver.as_bytes());
        hasher.update(&self.amount.to_be_bytes());
        for key in &self.ring_public_keys {
            hasher.update(key.as_bytes());
        }
        hasher.finalize().to_hex().to_string()
    }


    pub fn verify_signature(&self) -> bool {

        if self.sender_address == "SYSTEM" { return true; }
        

        let sig = match &self.signature {
            Some(s) => s,
            None => return false,
        };

        if sig.responses.len() != self.ring_public_keys.len() {
            return false;
        }


        if sig.responses.iter().any(|r| r.is_empty()) {
            return false;
        }

        let mut challenge_hasher = Hasher::new();
        challenge_hasher.update(self.tx_id.as_bytes());
        for key in &self.ring_public_keys {
            challenge_hasher.update(key.as_bytes());
        }
        let expected_challenge = challenge_hasher.finalize().to_hex().to_string();

        sig.challenge == expected_challenge
    }
}