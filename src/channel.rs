use crate::transaction::Transaction;
use crate::wallet::Wallet;

pub struct StateChannel {
    pub channel_id: String,
    pub participant_a: String,
    pub participant_b: String,
    pub balance_a: f64,
    pub balance_b: f64,
    pub local_history: Vec<Transaction>,
}

impl StateChannel {
    pub fn open(wallet_a: &Wallet, wallet_b: &Wallet, deposit_a: f64, deposit_b: f64) -> Self {
        let channel_id = blake3::hash(format!("{}{}", wallet_a.get_address(), wallet_b.get_address()).as_bytes())
            .to_hex()
            .to_string();

        println!("[SHAR 2] Відкрито ефемерну сесію каналу: {}", &channel_id[..10]);

        StateChannel {
            channel_id,
            participant_a: wallet_a.get_address(),
            participant_b: wallet_b.get_address(),
            balance_a: deposit_a,
            balance_b: deposit_b,
            local_history: Vec::new(),
        }
    }

    pub fn send_micro_transaction(&mut self, from_wallet: &Wallet, to_address: String, amount: f64) -> Option<Transaction> {
        let sender_address = from_wallet.get_address();

        if sender_address == self.participant_a {
            if self.balance_a < amount { return None; }
            self.balance_a -= amount;
            self.balance_b += amount;
        } else if sender_address == self.participant_b {
            if self.balance_b < amount { return None; }
            self.balance_b -= amount;
            self.balance_a += amount;
        } else {
            return None; 
        }

        let mut entropy = [0u8; 32];
        rand::RngCore::fill_bytes(&mut rand::rngs::OsRng, &mut entropy);
        let ephemeral_receiver = blake3::hash(format!("{}{}", to_address, hex::encode(entropy)).as_bytes())
            .to_hex()
            .to_string();

        let mut decoy_bytes = [0u8; 32];
        rand::RngCore::fill_bytes(&mut rand::rngs::OsRng, &mut decoy_bytes);
        let decoy = hex::encode(decoy_bytes);
        let ring_keys = vec![sender_address.clone(), decoy];

        let mut tx = Transaction::new(sender_address, ephemeral_receiver, amount, ring_keys.clone());
        
        // Модифікуємо tx_id під специфіку каналу, як і було раніше
        tx.tx_id = blake3::hash(format!("{}{}", tx.tx_id, self.channel_id).as_bytes())
            .to_hex()
            .to_string();

        // 🔥 Замість старого методу використовуємо кільцевий підпис
        let ring_sig = from_wallet.sign_ring(tx.tx_id.as_bytes(), &ring_keys);
        tx.signature = Some(ring_sig);

        self.local_history.push(tx.clone());
        println!("[SHAR 2 TX] Оперативний мікроплатіж зафіксовано в RAM: {} ZL.", amount);

        Some(tx)
    }

    pub fn close_and_settle(&mut self, from_wallet: &Wallet) -> Transaction {
        println!("[SHAR 2] Закриття каналу. Розрахунок фінального підсумку для Layer 1...");

        let sender = from_wallet.get_address();
        let receiver = if sender == self.participant_a {
            self.participant_b.clone()
        } else {
            self.participant_a.clone()
        };

        let final_amount = if receiver == self.participant_b {
            self.balance_b
        } else {
            self.balance_a
        };

        let mut entropy = [0u8; 32];
        rand::RngCore::fill_bytes(&mut rand::rngs::OsRng, &mut entropy);
        let final_ephemeral_receiver = blake3::hash(format!("{}{}", receiver, hex::encode(entropy)).as_bytes())
            .to_hex()
            .to_string();

        let mut decoy_bytes = [0u8; 32];
        rand::RngCore::fill_bytes(&mut rand::rngs::OsRng, &mut decoy_bytes);
        let decoy = hex::encode(decoy_bytes);
        let ring_keys = vec![sender.clone(), decoy];

        let mut settlement_tx = Transaction::new(
            sender,
            final_ephemeral_receiver,
            final_amount,
            ring_keys.clone(),
        );

        // 🔥 Замість старого методу використовуємо кільцевий підпис
        let ring_sig = from_wallet.sign_ring(settlement_tx.tx_id.as_bytes(), &ring_keys);
        settlement_tx.signature = Some(ring_sig);

        println!("[SHAR 2] Фінальний чек готов для відправки на Layer 1 двигун.");
        settlement_tx
    }
}