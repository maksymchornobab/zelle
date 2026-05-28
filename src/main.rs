mod blockchain;
mod transaction;
mod wallet;
mod channel;

use blockchain::Blockchain;
use transaction::Transaction;
use wallet::Wallet;

use std::env;
use std::io::{self, Write};
use std::sync::Arc;
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::sync::Mutex;

use weise::{WeiseTransport, MyBehaviourEvent};
use libp2p::futures::StreamExt; // Потрібно для циклу .select_next_some()
use libp2p::swarm::SwarmEvent; // Правильний імпорт для нових версій libp2p

#[tokio::main]
async fn main() {
    let args: Vec<String> = env::args().collect();
    if args.len() < 3 {
        println!("⚠️ Використання: \n  cargo run -- server developer\n  cargo run -- client alice");
        return;
    }

    let role = args[1].clone();
    let username = args[2].clone();

    // 1. Ініціалізуємо блокчейн
    let mut blockchain = Blockchain::new();
    
    // 2. Створюємо або завантажуємо гаманець
    let my_wallet = Wallet::load_or_create(&username);
    let my_address = my_wallet.get_address();

    if role == "server" && username == "developer" {
        // Запускаємо Генезис-блок у RAM автоматично
        if blockchain.init_genesis(my_address.clone()) {
            
            let (current_balance, bloom_filter) = my_wallet.get_state();
            
            if current_balance == 0.0 && bloom_filter.bits.iter().all(|&x| x == 0) {
                my_wallet.save_state(blockchain::MAX_SUPPLY, bloom_filter);
                println!("[💰 AUTOMINT] Оскільки це перший запуск, стартовий обсяг {} ZL автоматично записано на диск!", blockchain::MAX_SUPPLY);
            } else {
                println!("[💼 WALLET] Гаманець уже містить суверенну історію. Поточний баланс на диску: {} ZL", current_balance);
            }
        }
    }

    let zelle = Arc::new(Mutex::new(blockchain));
    let zelle_clone = Arc::clone(&zelle);
    let my_wallet_arc = Arc::new(my_wallet);
    let my_wallet_clone = Arc::clone(&my_wallet_arc);

    // --- P2P NETWORK LAYER ---
    // Ініціалізуємо анонімний P2P транспорт з твого SDK weise
    let mut transport = WeiseTransport::new().expect("Не вдалося запустити Weise SDK");
    println!("[🛰️ WEISE P2P ЗАПУЩЕНО] Мій Peer ID: {}", transport.local_peer_id);

    // Якщо користувач запустився як клієнт, підключаємо його до сервера
    if role == "client" {
        println!("[🔌 CONNECT] Введіть Multiaddr адресу сервера для підключення (наприклад, /ip4/127.0.0.1/tcp/XXXXX):");
        let mut server_addr_str = String::new();
        std::io::stdin().read_line(&mut server_addr_str).expect("Помилка зчитування");
        
        if let Ok(remote_addr) = server_addr_str.trim().parse::<libp2p::Multiaddr>() {
            let _ = transport.swarm.dial(remote_addr);
            println!("[🛰️] Надіслано запит на підключення до сервера...");
        } else {
            println!("[❌] Некоректний формат адреси!");
        }
    }

    // Налаштовуємо таймер для фоного стелс-шуму (Chaffing)
    let mut noise_timer = tokio::time::interval(tokio::time::Duration::from_secs(20));

    // Створюємо асинхронний канал зв'язку між терміналом команд та мережею weise
    let (tx_command, mut rx_command) = tokio::sync::mpsc::channel::<String>(32);
    let tx_command_clone = tx_command.clone();

    // Запускаємо єдиний асинхронний P2P потік
    tokio::spawn(async move {
        loop {
            tokio::select! {
                // Команда від нашого власного терміналу: забираємо її з каналу і анонімно шлемо в мережу
                // Команда від нашого власного терміналу
                Some(payload_to_send) = rx_command.recv() => {
                    if payload_to_send.starts_with("GARLIC_ROUTE:") {
                        // Розбиваємо рядок на 3 частини: "GARLIC_ROUTE", "receiver_pubkey", "корисні дані (TX:...)"
                        let parts: Vec<&str> = payload_to_send.splitn(3, ':').collect();
                        if parts.len() == 3 {
                            let receiver = parts[1];
                            let data = parts[2];
                            
                            // Викликаємо метод, який ми щойно дописали в garlic.rs!
                            let _ = transport.send_garlic(receiver, data);
                        }
                    } else {
                        // Для звичайних службових повідомлень або фонового шуму
                        let _ = transport.send_secure(&payload_to_send);
                    }
                }

                // Кожні 20 секунд автоматично шлемо 512-байтний пакет-шум для маскування трафіку
                _ = noise_timer.tick() => {
                    let _ = transport.send_noise();
                }

                // Обробляємо події P2P мережі
                event = transport.swarm.select_next_some() => match event {
                    SwarmEvent::NewListenAddr { address, .. } => {
                        println!("\n[🔥 МЕРЕЖА ОНЛАЙН] Твоя унікальна P2P-адреса:");
                        println!("👉 {} 👈", address);
                        println!("Поділися нею з іншим учасником мережі.\n");
                    }
                    
                    SwarmEvent::ConnectionEstablished { peer_id, .. } => {
                        println!("[🤝 P2P] Успішно з'єднано з піром: {}", peer_id);
                    }

                    SwarmEvent::Behaviour(MyBehaviourEvent::Gossipsub(libp2p::gossipsub::Event::Message { message, .. })) => {
                        if let Some(decoded_text) = WeiseTransport::unpack(&message.data) {
                            println!("[🕵️‍♂️ DEBUG 2] Успішний розпак (unpack)! Довжина тексту: {}", decoded_text.len());
                            if decoded_text.is_empty() { 
                                continue; 
                            }

                            if decoded_text.starts_with("GARLIC:") {
                                if let Ok(packet) = serde_json::from_str::<weise::garlic::GarlicPacket>(&decoded_text[7..]) {
                                    for clove in packet.cloves {
                                        if clove.next_hop == my_address {
                                            if let Ok(inner_text) = String::from_utf8(clove.encrypted_payload) {

                                                if inner_text.starts_with("TX:") {
                                                    if let Ok(tx) = serde_json::from_str::<Transaction>(&inner_text[3..]) {
                                                        zelle_clone.lock().await.add_transaction_to_mempool(tx);
                                                        println!("[🧄 GARLIC SUCCESS] Розпаковано часниковий зубчик! Анонімну транзакцію додано в мемпул!");
                                                    }
                                                }
                                            }
                                        }
                                    }
                                }
                            }


                            if decoded_text.starts_with("TX:") {
                                println!("[🕵️‍♂️ DEBUG] Розпізнано префікс TX:");
                                match serde_json::from_str::<Transaction>(&decoded_text[3..]) {
                                    Ok(tx) => {
                                        zelle_clone.lock().await.add_transaction_to_mempool(tx);
                                        println!("[🥷 WEISE SUCCESS] Спіймано анонімну транзакцію!");
                                    }
                                    Err(e) => println!("[❌ DEBUG ERROR] Помилка парсингу TX JSON: {}", e),
                                }
                             
                            } else if decoded_text.starts_with("MINED:") {
                                println!("[🕵️‍♂️ DEBUG] Розпізнано префікс MINED:");
                                match serde_json::from_str::<Vec<Transaction>>(&decoded_text[6..]) {
                                    Ok(txs) => {
                                        update_my_account_state(&my_wallet_clone, txs);
                                        println!("[🥷 WEISE SUCCESS] Спіймано і синхронізовано блок монет!");
                                    }
                                    Err(e) => println!("[❌ DEBUG ERROR] Помилка парсингу MINED JSON: {}", e),
                                }
                            } else {
                                println!("[❌ DEBUG] Невідомий префікс!");
                            }
                        } else {
                            println!("[❌ DEBUG ERROR] WeiseTransport::unpack повернув None! Пакет битий.");
                        }
                    }
                    _ => {}
                }
            }
        }
    });

    println!("МІЙ СУВЕРЕННИЙ АКАУНТ: {}", my_address);
    println!("Доступні команди: balance, send [адреса] [сума], mine");
    println!("==================================================");

    let mut stdin_lines = BufReader::new(tokio::io::stdin()).lines();

    loop {
        print!(">> ");
        io::stdout().flush().unwrap();

        if let Ok(Some(line)) = stdin_lines.next_line().await {
            let text = line.trim();

            if text == "balance" {
                let (bal, _) = my_wallet_arc.get_state();
                println!("[💰 ACCOUNT BALANCE] Твій суверенний баланс на диску: {} ZL", bal);
            } 
            
            else if text.starts_with("send ") {
                let parts: Vec<&str> = text.split_whitespace().collect();
                if parts.len() == 3 {
                    let receiver_pubkey = parts[1].to_string();
                    let amount = parts[2].parse::<f64>().unwrap_or(0.0);

                    let (mut bal, bloom_filter) = my_wallet_arc.get_state();

                    if bal < amount {
                        println!("[❌ ERROR] Недостатньо коштів на балансі! Доступно: {}", bal);
                        continue;
                    }

                    bal -= amount;
                    my_wallet_arc.save_state(bal, bloom_filter);

                    let mut entropy = [0u8; 32];
                    rand::RngCore::fill_bytes(&mut rand::rngs::OsRng, &mut entropy);
                    let ephemeral_receiver = blake3::hash(format!("{}{}", receiver_pubkey, hex::encode(entropy)).as_bytes())
                        .to_hex()
                        .to_string();

                    let mut decoy_bytes = [0u8; 32];
                    rand::RngCore::fill_bytes(&mut rand::rngs::OsRng, &mut decoy_bytes);
                    let decoy = hex::encode(decoy_bytes);
                    let ring_keys = vec![my_address.clone(), decoy];

                    let mut tx = Transaction::new(my_address.clone(), ephemeral_receiver, amount, ring_keys);
                    let sig = my_wallet_arc.sign(tx.tx_id.as_bytes());
                    tx.signature = sig.to_bytes().to_vec();

                    let mut bc = zelle.lock().await;
                    if bc.add_transaction_to_mempool(tx.clone()) {
                        if let Ok(tx_json) = serde_json::to_string(&tx) {
                            
                            // 🔥 НОВИЙ ЧАСНИКОВИЙ ШЛЯХ:
                            // Замість прямої відправки "TX:...", ми формуємо інструкцію для weise.
                            // Передаємо через двокрапку: GARLIC_ROUTE : [публічний_ключ_отримувача] : [JSON_транзакції]
                            let garlic_instruction = format!("GARLIC_ROUTE:{}:TX:{}", receiver_pubkey, tx_json);
                            
                            // Кидаємо інструкцію в канал, потік її підхопить
                            tx_command_clone.send(garlic_instruction).await.unwrap();
                            println!("[🧄 GARLIC QUEUED] Транзакцію запаковано в зубчик і поставлено в чергу часникового транспорту!");
                        }
                    }
                }
            } 
            
            else if text == "mine" {
                let mut bc = zelle.lock().await;
                let mined_txs = bc.mine_pending_transactions();
                if !mined_txs.is_empty() {
                    update_my_account_state(&my_wallet_arc, mined_txs.clone());
                    
                    if let Ok(txs_json) = serde_json::to_string(&mined_txs) {
                        
                        // Передаємо замайнений блок у канал для розсилки нодам
                        let mined_payload = format!("MINED:{}", txs_json);
                        tx_command_clone.send(mined_payload).await.unwrap();
                        println!("[⛏️ QUEUED] Блок замайнено та додано в чергу синхронізації!");
                    }
                }
            }
        }
    }
}

fn update_my_account_state(wallet: &Wallet, txs: Vec<Transaction>) {
    let (mut bal, mut bloom_filter) = wallet.get_state();
    let mut updated = false;

    for tx in txs {
        if tx.sender_address != wallet.get_address() {
            if !bloom_filter.contains(&tx.ephemeral_receiver) {
                bal += tx.amount;
                bloom_filter.insert(&tx.ephemeral_receiver);
                updated = true;
            } else {
                println!("[⚠️ WARNING] Фільтр Блума відхилив дублікат ефемерної адреси: {}!", &tx.ephemeral_receiver[..10]);
            }
        }
    }

    if updated {
        wallet.save_state(bal, bloom_filter);
        println!("[💼 WALLET] Прийнято нові ефемерні кошти. Фільтр Блума оновлено, баланс запечатано!");
    }
}