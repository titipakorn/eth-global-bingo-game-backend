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
use rocket::fairing::{Fairing, Info, Kind};
use rocket::http::Method;
use rocket::http::{Header, Status};
use rocket::Request;
use rocket::{get, launch, post, response::Response, routes, serde::json::Json, State};
use rocket_cors::{AllowedOrigins, CorsOptions};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::{str::FromStr, sync::Arc};
use tokio::sync::Mutex;
use tokio::time::{sleep, Duration};

// Background task control structure
pub struct BackgroundSubmitter {
    is_running: Arc<AtomicBool>,
}

impl BackgroundSubmitter {
    pub fn new() -> Self {
        Self {
            is_running: Arc::new(AtomicBool::new(true)),
        }
    }

    pub fn get_fairing(&self) -> BackgroundFairing {
        BackgroundFairing {
            is_running: self.is_running.clone(),
        }
    }

    pub fn stop(&self) {
        self.is_running.store(false, Ordering::SeqCst);
    }
}
#[rocket::async_trait]
impl Fairing for BackgroundFairing {
    fn info(&self) -> Info {
        Info {
            name: "Background Number Submitter",
            kind: Kind::Ignite,
        }
    }

    async fn on_ignite(
        &self,
        rocket: rocket::Rocket<rocket::Build>,
    ) -> Result<rocket::Rocket<rocket::Build>, rocket::Rocket<rocket::Build>> {
        let chain_states = rocket
            .state::<Arc<Mutex<HashMap<String, ChainState>>>>()
            .unwrap()
            .clone();
        let chain_states = chain_states.lock().await;
        for (chain_id, state) in chain_states.iter() {
            let app_state: AppState = state.app_state.clone();
            let contract = app_state.contract.clone();
            let is_running = self.is_running.clone();

            tokio::spawn({
                let chain_id = chain_id.clone();
                async move {
                    println!(
                        "Starting background number submission task for chain {}...",
                        chain_id
                    );
                    while is_running.load(Ordering::SeqCst) {
                        match submit_number(&contract).await {
                            Ok(_) => {
                                println!("Successfully submitted number for chain {}", chain_id)
                            }
                            Err(e) => {
                                eprintln!("Error submitting number for chain {}: {}", chain_id, e)
                            }
                        }
                        sleep(Duration::from_secs(15)).await;
                    }
                    println!("Background task stopped for chain {}", chain_id);
                }
            });
        }
        Ok(rocket)
    }
}

async fn submit_number(
    contract: &Arc<BingoGame<SignerMiddleware<Provider<Http>, LocalWallet>>>,
) -> Result<()> {
    let is_game_started = contract.is_game_started().call().await?;
    if is_game_started {
        // Generate random number between 1 and 99
        let mut rng = StdRng::from_entropy();
        let random_number = rng.gen::<u8>();
        let number = (random_number % 99) + 1;
        println!("Submitting number: {}", number);

        // Submit transaction
        let number_u256 = U256::from(number);
        let submit_call = contract.submit_drawn_number(number_u256);
        let tx = submit_call.send().await?;

        // Wait for confirmation
        let receipt = tx.await?;
        if let Some(receipt) = receipt {
            println!(
                "Number {} submitted in block: {:?}",
                number, receipt.block_number
            );
        } else {
            println!("Number {} submitted but receipt is None", number);
        }
    }

    Ok(())
}

// Rocket Fairing for background task
#[derive(Debug)]
pub struct BackgroundFairing {
    is_running: Arc<AtomicBool>,
}

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
        function submitDrawnNumber(uint256 number) external
        function assignCard(address player, uint256 randomSeed) external returns (uint32[25])
        function getCurrentGameState() external view returns (uint256 startTime, uint256 lastDrawTime, uint256 numberCount, uint256[] drawnNumbers, bool isEnded, uint256 playerCount, bool isStarted)
        function getPlayerCards(address player) external view returns (uint32[25] storedNumbers)
        function isGameStarted() external view returns (bool)
        function claimWin(address player) external returns (bool)
    ]"#
);

#[derive(Debug, Serialize)]
struct BingoCard {
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

#[derive(Debug, Clone)]
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
            let mut rng = StdRng::from_entropy();
            let random_number: U256 = U256::from(rng.gen::<u64>());

            println!("random numbers: {:?}", random_number);

            // Parse the wallet address from the request
            let parsed_address = request.walletAddress.parse::<Address>();
            let wallet_address = match &parsed_address {
                Ok(address) => address,
                Err(_) => {
                    eprintln!("Failed to parse wallet address: {}", request.walletAddress);
                    return Err(Status::BadRequest);
                }
            };

            // Assign the card to the given address
            match state
                .app_state
                .contract
                .assign_card(*wallet_address, random_number)
                .send()
                .await
            {
                Ok(tx) => match tx.await {
                    Ok(receipt) => {
                        let receipt = receipt.ok_or_else(|| {
                            eprintln!("Transaction receipt is None");
                            Status::InternalServerError
                        })?;
                        Ok(Json(ApiResponse {
                            success: true,
                            message: "Bingo card purchased and assigned successfully".to_string(),
                            data: Some(BingoCard {
                                transaction_hash: format!("{:?}", receipt.transaction_hash),
                            }),
                        }))
                    }
                    Err(e) => Ok(Json(ApiResponse {
                        success: false,
                        message: format!("Transaction failed: {}", e),
                        data: None,
                    })),
                },
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

#[post("/card/get/<chain_name>", format = "json", data = "<request>")]
async fn get_card(
    chain_name: String,
    request: Json<PurchaseCardRequest>,
    chain_states: &State<Arc<Mutex<HashMap<String, ChainState>>>>,
) -> Result<Json<ApiResponse<[u32; 25]>>, Status> {
    let chain_states = chain_states.lock().await;
    println!("Chain states: {:?}", chain_states);
    let state = chain_states.get(&chain_name).ok_or(Status::NotFound)?;
    print!("State: {:?}", state);
    // Parse the wallet address from the request
    let parsed_address = request.walletAddress.parse::<Address>();
    let wallet_address = match &parsed_address {
        Ok(address) => address,
        Err(_) => {
            eprintln!("Failed to parse wallet address: {}", request.walletAddress);
            return Err(Status::BadRequest);
        }
    };
    match state
        .app_state
        .contract
        .get_player_cards(*wallet_address)
        .call()
        .await
    {
        Ok(cards) => {
            let cards: [u32; 25] = cards;
            println!("cards: {:?}", cards);
            Ok(Json(ApiResponse {
                success: true,
                message: "Get Card".to_string(),
                data: Some(cards),
            }))
        }
        Err(e) => Ok(Json(ApiResponse {
            success: false,
            message: format!("Failed to get player cards: {}", e),
            data: None,
        })),
    }
}

#[post("/card/challenge/<chain_name>", format = "json", data = "<request>")]
async fn challenge(
    chain_name: String,
    request: Json<PurchaseCardRequest>,
    chain_states: &State<Arc<Mutex<HashMap<String, ChainState>>>>,
) -> Result<Json<ApiResponse<bool>>, Status> {
    let chain_states = chain_states.lock().await;
    let state = chain_states.get(&chain_name).ok_or(Status::NotFound)?;
    let parsed_address = request.walletAddress.parse::<Address>();
    let wallet_address = match &parsed_address {
        Ok(address) => address,
        Err(_) => {
            eprintln!("Failed to parse wallet address: {}", request.walletAddress);
            return Err(Status::BadRequest);
        }
    };
    match state
        .app_state
        .contract
        .claim_win(*wallet_address)
        .send()
        .await
    {
        Ok(tx) => match tx.await {
            Ok(_) => Ok(Json(ApiResponse {
                success: true,
                message: "You won!".to_string(),
                data: Some(true),
            })),
            Err(e) => Ok(Json(ApiResponse {
                success: false,
                message: format!("Invalid win {}", e),
                data: Some(false),
            })),
        },
        Err(e) => Ok(Json(ApiResponse {
            success: false,
            message: format!("Invalid win {}", e),
            data: Some(false),
        })),
    }
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

fn extract_card_numbers_from_receipt(receipt: &TransactionReceipt) -> Result<[u32; 25], String> {
    if let Some(log) = receipt.logs.get(0) {
        // Extract numbers from log data
        // This implementation depends on how your contract emits the card numbers
        // You'll need to adjust this based on your specific contract implementation
        if log.topics.len() > 1 {
            let numbers: Vec<u32> = log.topics[1]
                .as_bytes()
                .chunks(1)
                .map(|b| b[0] as u32)
                .collect();
            if numbers.len() == 25 {
                let mut card_numbers = [0u32; 25];
                card_numbers.copy_from_slice(&numbers);
                return Ok(card_numbers);
            }
        }
    }
    Err("Failed to extract card numbers from receipt".to_string())
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

    // Create the background submitter
    let background_submitter = BackgroundSubmitter::new();

    // Get the fairing
    let fairing = background_submitter.get_fairing();

    let cors = CorsOptions::default()
        .allowed_origins(AllowedOrigins::all())
        .allowed_methods(
            vec![Method::Get, Method::Post, Method::Patch]
                .into_iter()
                .map(From::from)
                .collect(),
        )
        .allow_credentials(true);

    // Launch Rocket
    rocket::build()
        .manage(chain_states)
        .attach(cors.to_cors().unwrap())
        .mount(
            "/api",
            routes![get_game_state, purchase_card, get_card, challenge],
        )
        .attach(fairing)
}

// Graceful shutdown handler (add this to your main function if you're not using #[launch])
pub async fn shutdown_handler(background_submitter: &BackgroundSubmitter) {
    tokio::signal::ctrl_c()
        .await
        .expect("Failed to listen for ctrl-c");
    println!("Shutdown signal received, stopping background task...");
    background_submitter.stop();
}
