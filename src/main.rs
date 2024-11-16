use dotenv::dotenv;
use ethers::types::Address;
use ethers::{
    prelude::*,
    providers::{Http, Provider},
    signers::{LocalWallet, Signer},
};
use eyre::Result;
use rand::rngs::StdRng;
use rand::{Rng, SeedableRng};
use rocket::http::Status;
use rocket::{get, launch, post, routes, serde::json::Json, State};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::{str::FromStr, sync::Arc};
use tokio::sync::Mutex;

// Define a struct to hold the state for each chain
#[derive(Debug)]
struct ChainState {
    rpc_url: String,
    contract_address: String,
    private_key: String,
    app_state: AppState,
}

impl ChainState {
    async fn new(
        rpc_url: &str,
        contract_address: &str,
        private_key: &str,
    ) -> Result<Self, Box<dyn std::error::Error>> {
        let app_state = AppState::new(rpc_url, contract_address, private_key).await?;
        Ok(Self {
            rpc_url: rpc_url.to_string(),
            contract_address: contract_address.to_string(),
            private_key: private_key.to_string(),
            app_state,
        })
    }
}

// Contract ABI definition
abigen!(
    BingoGame,
    r#"[
        function assignCard(address player, uint8[25] cardNumbers) external
        function getCurrentGameState() external view returns (uint256 startTime, uint256 lastDrawTime, uint256 numberCount, uint256[] drawnNumbers, bool isEnded, uint256 playerCount, bool isStarted)
    ]"#
);

#[derive(Debug, Serialize)]
struct BingoCard {
    numbers: [[u8; 5]; 5], // 5x5 Bingo card
    transaction_hash: String,
}

#[derive(Debug, Serialize)]
struct GameState {
    start_time: u64,
    last_draw_time: u64,
    drawn_numbers_count: i8,
    drawn_numbers: Vec<i8>,
    is_ended: bool,
    player_count: i32,
    is_started: bool,
}

#[derive(Debug, Serialize)]
struct ApiResponse<T> {
    success: bool,
    message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    data: Option<T>,
}

#[derive(Debug)]
struct AppState {
    contract: Arc<BingoGame<SignerMiddleware<Provider<Http>, LocalWallet>>>,
}

impl AppState {
    async fn new(rpc_url: &str, contract_address: &str, private_key: &str) -> Result<Self> {
        let provider: Provider<Http> = Provider::<Http>::try_from(rpc_url)?;
        let chain_id = provider.get_chainid().await?.as_u64();

        let wallet = LocalWallet::from_str(private_key)?.with_chain_id(chain_id);
        let client = SignerMiddleware::new(provider, wallet);
        let client = Arc::new(client);

        let contract_address = Address::from_str(contract_address)?;
        let contract = BingoGame::new(contract_address, client);
        let contract = Arc::new(contract);

        Ok(Self { contract })
    }
}

#[derive(Debug, Deserialize)]
struct PurchaseCardRequest {
    walletAddress: String,
}

#[get("/game/state/<chain_name>")]
async fn get_game_state(
    chain_name: &str,
    chain_states: &State<Arc<Mutex<HashMap<String, ChainState>>>>,
) -> Result<Json<ApiResponse<GameState>>, Status> {
    let chain_states = chain_states.lock().await;
    println!("Chain states: {:?}", chain_states);
    let state = chain_states.get(chain_name).ok_or(Status::NotFound)?;
    print!("State: {:?}", state);

    match state
        .app_state
        .contract
        .get_current_game_state()
        .call()
        .await
    {
        Ok((
            start_time,
            last_draw_time,
            number_count,
            drawn_numbers,
            is_ended,
            player_count,
            is_started,
        )) => {
            let game_state = GameState {
                start_time: start_time.as_u64(),
                last_draw_time: last_draw_time.as_u64(),
                drawn_numbers_count: number_count.as_u32() as i8,
                drawn_numbers: drawn_numbers.iter().map(|n| n.as_u32() as i8).collect(),
                is_ended,
                player_count: player_count.as_u32() as i32,
                is_started,
            };

            Ok(Json(ApiResponse {
                success: true,
                message: format_game_status_message(&game_state),
                data: Some(game_state),
            }))
        }
        Err(e) => Ok(Json(ApiResponse {
            success: false,
            message: format!("Failed to get game state: {}", e),
            data: None,
        })),
    }
}

#[post("/card/purchase/<chain_name>", format = "json", data = "<request>")]
async fn purchase_card(
    chain_name: String,
    request: Json<PurchaseCardRequest>,
    chain_states: &State<Arc<Mutex<HashMap<String, ChainState>>>>,
) -> Result<Json<ApiResponse<BingoCard>>, Status> {
    let chain_states = chain_states.lock().await;
    println!("Chain states: {:?}", chain_states);
    let state = chain_states.get(&chain_name).ok_or(Status::NotFound)?;
    print!("State: {:?}", state);
    match state
        .app_state
        .contract
        .get_current_game_state()
        .call()
        .await
    {
        Ok((_, _, _, _, _, _, is_started)) => {
            if is_started {
                return Ok(Json(ApiResponse {
                    success: false,
                    message: "Game has already started".to_string(),
                    data: None,
                }));
            }
            // let mut rng = StdRng::from_entropy();
            // let mut card_numbers = [0u8; 25];
            // for i in 0..25 {
            //     card_numbers[i] = rng.gen_range(1..=75);
            // }
            // card_numbers[12] = 0; // Mark the center number as free

            // Fixed value for card_numbers
            let card_numbers: [u8; 25] = [
                1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 0, 14, 15, 16, 17, 18, 19, 20, 21, 22, 23,
                24, 25,
            ]; // Example values, adjust as needed

            println!("Card numbers: {:?}", card_numbers);

            // Parse the wallet address from the request
            let parsed_address = request.walletAddress.parse::<Address>();
            let wallet_address = match &parsed_address {
                Ok(address) => address,
                Err(_) => {
                    eprintln!("Failed to parse wallet address: {}", request.walletAddress);
                    return Err(Status::BadRequest);
                }
            };

            println!("Wallet address: {:?}", wallet_address);

            // Assign the card to the given address
            match state
                .app_state
                .contract
                .assign_card(*wallet_address, card_numbers)
                .send()
                .await
            {
                Ok(tx) => {
                    match tx.await {
                        Ok(receipt) => {
                            // Convert the array of 25 numbers into a 5x5 grid
                            let numbers = parse_card_numbers(receipt.as_ref().unwrap());

                            Ok(Json(ApiResponse {
                                success: true,
                                message: "Bingo card purchased and assigned successfully"
                                    .to_string(),
                                data: Some(BingoCard {
                                    numbers,
                                    transaction_hash: format!(
                                        "{:?}",
                                        receipt.unwrap().transaction_hash
                                    ),
                                }),
                            }))
                        }
                        Err(e) => Ok(Json(ApiResponse {
                            success: false,
                            message: format!("Transaction failed: {}", e),
                            data: None,
                        })),
                    }
                }
                Err(e) => Ok(Json(ApiResponse {
                    success: false,
                    message: format!("Failed to send transaction: {}", e),
                    data: None,
                })),
            }
        }
        Err(e) => Ok(Json(ApiResponse {
            success: false,
            message: format!("Failed to check game state: {}", e),
            data: None,
        })),
    }
}

// Helper function to format timestamps
fn format_timestamp(timestamp: u64) -> String {
    use chrono::{DateTime, NaiveDateTime, Utc};
    let naive = NaiveDateTime::from_timestamp_opt(timestamp as i64, 0).unwrap_or_default();
    let datetime: DateTime<Utc> = DateTime::from_naive_utc_and_offset(naive, Utc);
    datetime.format("%Y-%m-%d %H:%M:%S UTC").to_string()
}

// Helper function to format game status message
fn format_game_status_message(state: &GameState) -> String {
    if (!state.is_started) {
        "Game has not started yet".to_string()
    } else if (state.is_ended) {
        "Game has ended".to_string()
    } else {
        format!(
            "Game is active with {} players. {} numbers drawn so far",
            state.player_count, state.drawn_numbers_count
        )
    }
}

// Helper function to parse card numbers from transaction receipt
fn parse_card_numbers(receipt: &TransactionReceipt) -> [[u8; 5]; 5] {
    let mut card = [[0u8; 5]; 5];
    if let Some(log) = receipt.logs.get(0) {
        // Extract numbers from log data
        // This implementation depends on how your contract emits the card numbers
        // You'll need to adjust this based on your specific contract implementation
        if log.topics.len() > 1 {
            let numbers: Vec<u8> = log.topics[1].as_bytes().chunks(1).map(|b| b[0]).collect();

            // Convert flat array to 5x5 grid
            for (i, chunk) in numbers.chunks(5).enumerate() {
                if i < 5 {
                    for (j, &num) in chunk.iter().enumerate() {
                        if j < 5 {
                            card[i][j] = num;
                        }
                    }
                }
            }
        }
    }
    card
}

#[launch]
async fn rocket() -> _ {
    dotenv().ok();
    // Configuration
    let rpc_urls = std::env::var("RPC_URLS").expect("RPC_URLS must be set");
    let contract_addresses =
        std::env::var("CONTRACT_ADDRESSES").expect("CONTRACT_ADDRESSES must be set");
    let private_keys = std::env::var("PRIVATE_KEYS").expect("PRIVATE_KEYS must be set");
    let chain_names = std::env::var("CHAIN_NAMES").expect("CHAIN_NAMES must be set");

    let rpc_urls: Vec<&str> = rpc_urls.split(',').collect();
    let contract_addresses: Vec<&str> = contract_addresses.split(',').collect();
    let private_keys: Vec<&str> = private_keys.split(',').collect();
    let chain_names: Vec<&str> = chain_names.split(',').collect();

    if rpc_urls.len() != contract_addresses.len()
        || rpc_urls.len() != private_keys.len()
        || rpc_urls.len() != chain_names.len()
    {
        panic!(
            "RPC_URLS, CONTRACT_ADDRESSES, PRIVATE_KEYS, and CHAIN_NAMES must have the same length"
        );
    }
    // Initialize app state for each chain
    let mut chain_states = HashMap::new();
    for i in 0..rpc_urls.len() {
        let chain_state = ChainState::new(rpc_urls[i], contract_addresses[i], private_keys[i])
            .await
            .expect("Failed to initialize chain state");
        chain_states.insert(chain_names[i].to_string(), chain_state);
    }

    // Wrap the chain states in an Arc and Mutex for shared access
    let chain_states = Arc::new(Mutex::new(chain_states));

    // Launch Rocket
    rocket::build()
        .manage(chain_states)
        .mount("/api", routes![get_game_state, purchase_card])
}