use ed25519_dalek::{SigningKey, VerifyingKey, Signature, Signer};
use rand::RngCore;
use rand::rngs::OsRng;
use std::fs::File;
use std::io::{Read, Write, stdin, stdout};
use std::path::Path;
use serde::{Serialize, Deserialize};
use blake3::Hasher;
use chacha20poly1305::{ChaCha20Poly1305, Key, Nonce};
use chacha20poly1305::aead::{Aead, KeyInit};

// Імпортуємо твій контейнер захисту пам'яті
use verteidiger::SecureSeed; 

const BLOOM_FILTER_BYTES: usize = 32 * 1024; 
const BLOOM_HASH_FUNCTIONS: u8 = 7;

#[derive(Serialize, Deserialize, Clone)]
pub struct BloomFilter {
    pub bits: Vec<u8>,
}

impl BloomFilter {
    pub fn new() -> Self {
        BloomFilter { bits: vec![0u8; BLOOM_FILTER_BYTES] }
    }

    pub fn insert(&mut self, key: &str) {
        let total_bits = BLOOM_FILTER_BYTES * 8;
        for i in 0..BLOOM_HASH_FUNCTIONS {
            let mut hasher = Hasher::new();
            hasher.update(&[i]);
            hasher.update(key.as_bytes());
            let hash = hasher.finalize();
            
            let mut bytes = [0u8; 8];
            bytes.copy_from_slice(&hash.as_bytes()[0..8]);
            let bit_index = (u64::from_be_bytes(bytes) % total_bits as u64) as usize;

            let byte_pos = bit_index / 8;
            let bit_pos = bit_index % 8;
            self.bits[byte_pos] |= 1 << bit_pos;
        }
    }

    pub fn contains(&self, key: &str) -> bool {
        let total_bits = BLOOM_FILTER_BYTES * 8;
        for i in 0..BLOOM_HASH_FUNCTIONS {
            let mut hasher = Hasher::new();
            hasher.update(&[i]);
            hasher.update(key.as_bytes());
            let hash = hasher.finalize();

            let mut bytes = [0u8; 8];
            bytes.copy_from_slice(&hash.as_bytes()[0..8]);
            let bit_index = (u64::from_be_bytes(bytes) % total_bits as u64) as usize;

            let byte_pos = bit_index / 8;
            let bit_pos = bit_index % 8;

            if (self.bits[byte_pos] & (1 << bit_pos)) == 0 {
                return false;
            }
        }
        true
    }
}

#[derive(Serialize, Deserialize, Clone)]
struct WalletPayload {
    pub secret_seed_hex: String,
    pub balance: f64,
    pub bloom_filter: BloomFilter,
}

#[derive(Serialize, Deserialize, Clone)]
struct EncryptedWalletFile {
    pub salt_hex: String,
    pub nonce_hex: String,
    pub ciphertext_hex: String,
}

// 🛡️ Оновлена структура гаманця
pub struct Wallet {
    // signing_key видалено звідси! Ключ тепер надійно схований всередині secret_seed
    pub secret_seed: SecureSeed, 
    pub verifying_key: VerifyingKey,
    pub username: String,
    encryption_key: [u8; 32], 
}

impl Wallet {
    pub fn load_or_create(username: &str) -> Self {
        let filename = format!("wallets/{}_wallet.json", username);
        let path = Path::new(&filename);

        if path.exists() {
            println!("\n🔑 [ЗАХИСТ] Гаманець зашифровано. Введіть пароль для доступу:");
            let password = Wallet::read_password_from_console();

            let mut file = File::open(path).expect("Не вдалося відкрити файл");
            let mut contents = String::new();
            file.read_to_string(&mut contents).unwrap();
            
            let encrypted_data: EncryptedWalletFile = serde_json::from_str(&contents)
                .expect("Файл гаманця пошкоджено на диску!");

            let salt = hex::decode(&encrypted_data.salt_hex).unwrap();
            let encryption_key = Wallet::derive_key_from_password(&password, &salt);

            let cipher = ChaCha20Poly1305::new(Key::from_slice(&encryption_key));
            
            let nonce_bytes = hex::decode(&encrypted_data.nonce_hex).unwrap();
            let nonce = Nonce::from_slice(&nonce_bytes);
            let ciphertext = hex::decode(&encrypted_data.ciphertext_hex).unwrap();

            let decrypted_bytes = cipher.decrypt(nonce, ciphertext.as_ref())
                .map_err(|_| panic!("🚨 АВАРІЙНИЙ БАН: Неправильний пароль або вміст файлу було підроблено/модифіковано хакером!"))
                .unwrap();

            let payload: WalletPayload = serde_json::from_slice(&decrypted_bytes).unwrap();

            let seed_bytes = hex::decode(&payload.secret_seed_hex).unwrap();
            
            // 🛡️ Крок 1: Отримуємо фіксований масив байтів для ініціалізації
            let mut raw_seed = [0u8; 32];
            raw_seed.copy_from_slice(&seed_bytes[..32]);

            // 🛡️ Крок 2: Одразу ініціалізуємо SigningKey суто для деривації публічного ключа
            let signing_key = SigningKey::from_bytes(&raw_seed);
            let verifying_key = VerifyingKey::from(&signing_key);

            // 🛡️ Крок 3: Загортаємо сирі байти в безпечний контейнер RAM Hardening
            let secret_seed = SecureSeed::new(raw_seed);

            println!("[🔓 SUCCESS] Гаманець успішно дешифровано, приватний ключ запечатано в захищену RAM.");

            Wallet { secret_seed, verifying_key, username: username.to_string(), encryption_key }
        } else {
            println!("\n🆕 [СТВОРЕННЯ ГАМАНЦЯ] Встановіть надійний пароль для файлу `{}`:", filename);
            let password = Wallet::read_password_from_console();

            let mut seed = [0u8; 32];
            OsRng.fill_bytes(&mut seed);
            
            let signing_key = SigningKey::from_bytes(&seed);
            let verifying_key = VerifyingKey::from(&signing_key);
            let seed_hex = hex::encode(seed);

            let mut salt = [0u8; 16];
            OsRng.fill_bytes(&mut salt);

            let encryption_key = Wallet::derive_key_from_password(&password, &salt);
            
            let payload = WalletPayload {
                secret_seed_hex: seed_hex,
                balance: 0.0,
                bloom_filter: BloomFilter::new(),
            };

            // 🛡️ Загортаємо згенерований сид у безпечний контейнер
            let secret_seed = SecureSeed::new(seed);

            let mut wallet = Wallet { 
                secret_seed, 
                verifying_key, 
                username: username.to_string(), 
                encryption_key 
            };

            wallet.write_encrypted_to_disk(payload, &salt);
            println!("[💾 SECURE] Новий гаманець створено та запечатано шифром ChaCha20-Poly1305!");



            wallet
        }
    }

    pub fn get_address(&self) -> String { hex::encode(self.verifying_key.to_bytes()) }
    
    // 🛡️ Нова безпечна функція підпису! Вона відкриває ключ лише всередині замикання
    pub fn sign(&self, message: &[u8]) -> Signature { 
        self.secret_seed.use_seed(|raw_seed_bytes| {
            let signing_key = SigningKey::from_bytes(raw_seed_bytes);
            signing_key.sign(message)
        }) // <--- Тут raw_seed_bytes автоматично знищується і затирається нулями
    }

    pub fn get_state(&self) -> (f64, BloomFilter) {
        let filename = format!("wallets/{}_wallet.json", self.username);
        let mut file = File::open(filename).unwrap();
        let mut contents = String::new();
        file.read_to_string(&mut contents).unwrap();
        
        let encrypted_data: EncryptedWalletFile = serde_json::from_str(&contents).unwrap();
        let cipher = ChaCha20Poly1305::new(Key::from_slice(&self.encryption_key));
        
        let nonce_bytes = hex::decode(&encrypted_data.nonce_hex).unwrap();
        let nonce = Nonce::from_slice(&nonce_bytes);
        let ciphertext = hex::decode(&encrypted_data.ciphertext_hex).unwrap();

        let decrypted_bytes = cipher.decrypt(nonce, ciphertext.as_ref()).unwrap();
        let payload: WalletPayload = serde_json::from_slice(&decrypted_bytes).unwrap();

        (payload.balance, payload.bloom_filter)
    }

    pub fn save_state(&self, balance: f64, bloom_filter: BloomFilter) {
        let filename = format!("wallets/{}_wallet.json", self.username);
        let mut file = File::open(&filename).unwrap();
        let mut contents = String::new();
        file.read_to_string(&mut contents).unwrap();
        let encrypted_data: EncryptedWalletFile = serde_json::from_str(&contents).unwrap();
        let salt = hex::decode(&encrypted_data.salt_hex).unwrap();

        let cipher = ChaCha20Poly1305::new(Key::from_slice(&self.encryption_key));
        
        let nonce_bytes = hex::decode(&encrypted_data.nonce_hex).unwrap();
        let nonce = Nonce::from_slice(&nonce_bytes);
        
        let ciphertext = hex::decode(&encrypted_data.ciphertext_hex).unwrap();
        let decrypted_bytes = cipher.decrypt(nonce, ciphertext.as_ref()).unwrap();
        let old_payload: WalletPayload = serde_json::from_slice(&decrypted_bytes).unwrap();

        let new_payload = WalletPayload {
            secret_seed_hex: old_payload.secret_seed_hex,
            balance,
            bloom_filter,
        };

        // Тимчасова заглушка для перезапису стану не потребує справжнього ключа
        let mut wallet_clone = Wallet {
            secret_seed: SecureSeed::new([0u8; 32]), 
            verifying_key: self.verifying_key,
            username: self.username.clone(),
            encryption_key: self.encryption_key,
        };
        wallet_clone.write_encrypted_to_disk(new_payload, &salt);
    }

    fn write_encrypted_to_disk(&mut self, payload: WalletPayload, salt: &[u8]) {
        let filename = format!("wallets/{}_wallet.json", self.username);
        
        let mut nonce_bytes = [0u8; 12];
        OsRng.fill_bytes(&mut nonce_bytes);
        let nonce = Nonce::from_slice(&nonce_bytes);

        let cipher = ChaCha20Poly1305::new(Key::from_slice(&self.encryption_key));
        let payload_bytes = serde_json::to_vec(&payload).unwrap();
        
        let ciphertext = cipher.encrypt(nonce, payload_bytes.as_ref())
            .expect("Помилка шифрування даних!");

        let encrypted_file = EncryptedWalletFile {
            salt_hex: hex::encode(salt),
            nonce_hex: hex::encode(nonce_bytes),
            ciphertext_hex: hex::encode(ciphertext),
        };

        let json_str = serde_json::to_string_pretty(&encrypted_file).unwrap();
        File::create(filename).unwrap().write_all(json_str.as_bytes()).unwrap();
    }

    fn derive_key_from_password(password: &str, salt: &[u8]) -> [u8; 32] {
        let mut hasher = Hasher::new();
        hasher.update(salt);
        hasher.update(password.as_bytes());
        *hasher.finalize().as_bytes()
    }

    fn read_password_from_console() -> String {
        let mut password = String::new();
        stdout().flush().unwrap();
        stdin().read_line(&mut password).expect("Не вдалося зчитати рядок");
        password.trim().to_string()
    }
}