# Bingo Game from MiniGames Club

This project is a decentralized Bingo Game where everyone can play across multiple chains together. The backend is written in Rust, and the smart contract is written in Solidity.

## Project Structure

- **Rust Backend**: Handles the game logic and interactions with the blockchain.
- **Solidity Smart Contract**: Manages the game state and ensures fairness and transparency.

## Prerequisites

- Rust (latest stable version)
- Node.js (for interacting with the smart contract)
- Solidity compiler (solc)

## Installation

1. Clone the repository:

   ```sh
   git clone https://github.com/yourusername/bingo-game.git
   cd bingo-game
   ```

2. Install Rust dependencies:

   ```sh
   cargo build
   ```

3. Install Node.js dependencies:

   ```sh
   npm install
   ```

4. Compile the Solidity smart contract:
   ```sh
   npx hardhat compile
   ```

## Usage

1. Start the Rust backend:

   ```sh
   cargo run
   ```

2. Deploy the smart contract:

   ```sh
   npx hardhat run scripts/deploy.js
   ```

3. Interact with the game through the provided frontend or via command line tools.

## Contributing

Contributions are welcome! Please open an issue or submit a pull request.

## License

This project is licensed under the MIT License.
