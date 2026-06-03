use crate::transaction::Transaction;
use std::collections::HashSet;

pub const MAX_SUPPLY: f64 = 1_000_000.0;

pub struct Block {
    pub index: u64,
    pub timestamp: u64,
    pub transactions: Vec<Transaction>,
    pub prev_hash: String,
    pub hash: String,
    pub nonce: u64,
}

pub struct Blockchain {
    pub chain: Vec<Block>,
    pub mempool: Vec<Transaction>,
    pub difficulty: usize,
    pub spent_ephemeral_addresses: HashSet<String>,
    pub is_genesis_minted: bool,
}

impl Blockchain {
    pub fn new() -> Self {
        Blockchain {
            chain: Vec::new(),
            mempool: Vec::new(),
            difficulty: 3,
            spent_ephemeral_addresses: HashSet::new(),
            is_genesis_minted: false,
        }
    }

    pub fn init_genesis(&mut self, developer_address: String) -> bool {
        if self.is_genesis_minted || !self.chain.is_empty() {
            println!("[❌ CONSENSUS ERROR] Критичне порушення: спроба повторного мінту токенів!");
            return false;
        }

        let genesis_tx = Transaction::new(
            "SYSTEM".to_string(),
            developer_address,
            MAX_SUPPLY, 
            vec!["GENESIS_POINT".to_string()], 
        );

        let mut genesis_block = Block {
            index: 0,
            timestamp: 1680000000,
            transactions: vec![genesis_tx],
            prev_hash: "0".repeat(64),
            hash: String::new(),
            nonce: 0,
        };

        genesis_block.hash = Blockchain::calculate_block_hash(&genesis_block);
        self.chain.push(genesis_block);
        self.is_genesis_minted = true;
        
        println!("[🛰️ CONSTITUTION] Первинну емісію активовано. Фіксований Supply: {} ZL. Мінт назавжди ЗАБЛОКОВАНО.", MAX_SUPPLY);
        true
    }

    pub fn add_transaction_to_mempool(&mut self, tx: Transaction) -> bool {
        // 🔥 Тут викликається внутрішня перевірка кільця з transaction.rs
        if !tx.verify_signature() {
            println!("[VALIDATOR ERROR] Відхилено: Зламаний кільцевий підпис або підроблені дані!");
            return false;
        }

        if self.spent_ephemeral_addresses.contains(&tx.ephemeral_receiver) {
            println!("[🚫 VALIDATOR ERROR] Відхилено: Ефемерна точка {} вже була погашена!", tx.ephemeral_receiver);
            return false;
        }

        if tx.sender_address == "SYSTEM" && self.is_genesis_minted {
            println!("[❌ VALIDATOR ERROR] Відхилено: Спроба нелегальної емісії через SYSTEM!");
            return false;
        }

        println!("[MEMPOOL] Валідація успішна. Кільцеву транзакцію додано в RAM-вузол: {} ZL", tx.amount);
        self.mempool.push(tx);
        true
    }

    pub fn mine_pending_transactions(&mut self) -> Vec<Transaction> {
        if self.mempool.is_empty() { return Vec::new(); }

        let last_block = self.chain.last().unwrap().clone();
        let mut new_block = Block {
            index: last_block.index + 1,
            timestamp: 1779791924, 
            transactions: self.mempool.clone(),
            prev_hash: last_block.hash.clone(),
            hash: String::new(),
            nonce: 0,
        };

        let target = "0".repeat(self.difficulty);
        loop {
            let hash = Blockchain::calculate_block_hash(&new_block);
            if hash.starts_with(&target) {
                new_block.hash = hash;
                break;
            }
            new_block.nonce += 1;
        }

        println!("[MINER] Блок #{} успішно замайнено в RAM двигуна!", new_block.index);
        
        for tx in &self.mempool {
            self.spent_ephemeral_addresses.insert(tx.ephemeral_receiver.clone());
        }

        let mined_txs = self.mempool.clone();
        self.chain.push(new_block);
        self.mempool.clear();
        mined_txs
    }

    fn calculate_block_hash(block: &Block) -> String {
        let mut hasher = blake3::Hasher::new();
        hasher.update(&block.index.to_be_bytes());
        hasher.update(&block.timestamp.to_be_bytes());
        hasher.update(&block.nonce.to_be_bytes());
        hasher.update(block.prev_hash.as_bytes());
        let txs_json = serde_json::to_string(&block.transactions).unwrap_or_default();
        hasher.update(txs_json.as_bytes());
        hasher.finalize().to_hex().to_string()
    }
}