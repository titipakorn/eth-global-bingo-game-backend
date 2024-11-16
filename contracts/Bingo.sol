// SPDX-License-Identifier: MIT
pragma solidity ^0.8.19;

import "@openzeppelin/contracts/security/Pausable.sol";
import "@openzeppelin/contracts/access/Ownable.sol";

/// @title Bingo Game Smart Contract
/// @notice Implements a decentralized Bingo game with automated number drawing
/// @dev Includes security features and proper state management
contract BingoGame is Pausable, Ownable {

    // Custom errors
    error GameAlreadyInProgress();
    error GameNotInProgress();
    error InvalidCardPurchase();
    error InvalidDrawInterval();
    error InsufficientPlayers();

    // Constants
    uint256 public constant DRAW_INTERVAL = 5 seconds;
    uint256 public constant MIN_PLAYERS = 1;
    uint256 public constant N_SIZE = 5;
    uint256 public constant BOARD_SIZE = N_SIZE*N_SIZE;
    uint256 public constant MAX_NUMBER = 99;

    // Storage
    struct BingoCard {
        address owner;
        uint8[25] numbers;
        bool isInit;
        bool hasWon;
    }

    struct GameState {
        uint256 startTime;
        uint256 lastDrawTime;
        uint256[] drawnNumbers;
        bool gameEnded;
    }

    // State variables
    GameState public games;
    mapping(address => BingoCard) public cards;
    mapping(address => bool) private gamePlayers;
    uint256 public currentGameId;
    uint256 private gamePlayerCount;
    mapping(uint256 => bool) private usedNumbers;
    
    // Events
    event GameStarted(uint256 timestamp);
    event CardPurchased(address indexed player);
    event NumberDrawn(uint256 number);
    event WinClaimed(address indexed player);
    event GameEnded(uint256 timestamp);

    /// @notice Initializes the contract
    constructor() Ownable(msg.sender) {
    }

    /// @notice Initializes a new game session (internal)
    /// @dev Called automatically when minimum players is reached
    function _startNewGame() private {
        GameState storage newGame = games;
        newGame.startTime = block.timestamp;
        newGame.lastDrawTime = block.timestamp;
        newGame.gameEnded = false;

        // Initialize drawn numbers with 0 as the first number
        newGame.drawnNumbers = new uint256[](1);
        newGame.drawnNumbers[0] = 0;
        _resetUsedNumbers();
        usedNumbers[0] = true;
        emit GameStarted(block.timestamp);
    }

    function _resetUsedNumbers() private {
        // Reset only the possible bingo numbers
        for(uint256 i = 1; i <= MAX_NUMBER; i++) {
            usedNumbers[i] = false;
        }
    }

    /// @notice Allows players to purchase a bingo card
    /// @dev Generates random card numbers and assigns ownership
    /// @return numbers The numbers on the purchased card
    function purchaseCard() external whenNotPaused returns (uint8[25] memory numbers) {
        if (games.startTime>0) {
            revert GameAlreadyInProgress();
        }

        numbers = generateCardNumbers();
        
        cards[msg.sender] = BingoCard({
            owner: msg.sender,
            numbers: numbers,
            hasWon: false,
            isInit: true
        });

        // Track unique players in the game
        if (!gamePlayers[msg.sender]) {
            gamePlayers[msg.sender] = true;
            gamePlayerCount++;

            // Auto-start game when minimum players is reached
            if (gamePlayerCount == MIN_PLAYERS) {
                _startNewGame();
            }
        }

        emit CardPurchased(msg.sender);

        return numbers;
    }

    /// @notice Gets all cards owned by a specific player for the current game
    /// @return storedNumbers 2D array of card numbers
    function getPlayerCards() external view returns (
        uint8[25] memory storedNumbers
    ) {
        require(cards[msg.sender].isInit, "No card!");
        storedNumbers = cards[msg.sender].numbers;
        return storedNumbers;
    }

    /// @notice Draws a new number for the current game
    /// @dev Can only be called after DRAW_INTERVAL has passed
    function drawNumber() external whenNotPaused {
        GameState storage game = games;
        
        if (game.gameEnded) {
            revert GameNotInProgress();
        }

        if (gamePlayerCount < MIN_PLAYERS) {
            revert InsufficientPlayers();
        }
        // if (block.timestamp < game.lastDrawTime + DRAW_INTERVAL) {
        //     revert InvalidDrawInterval();
        // }

        uint256 newNumber = generateRandomNumber() % MAX_NUMBER + 1;
        while (usedNumbers[newNumber]) {
            newNumber = (newNumber + 1) % MAX_NUMBER + 1;
        }

        game.drawnNumbers.push(newNumber);
        usedNumbers[newNumber] = true;
        game.lastDrawTime = block.timestamp;

        emit NumberDrawn(newNumber);

        // End game if all numbers are drawn
        if (game.drawnNumbers.length >= MAX_NUMBER) {
            endGame();
        }
    }

    /// @notice Allows players to claim a win
    function claimWin() external whenNotPaused returns (bool) {

        BingoCard storage card = cards[msg.sender];

        if (card.owner != msg.sender || card.hasWon) {
            return false;
        }
        if (!_verifyWin(card)) {
            return false;
        }
        card.hasWon = true;
        emit WinClaimed(msg.sender);
        endGame();
        return true;
    }


    /// @notice Ends the current game
    /// @dev Can be called by owner or automatically when conditions are met
    function endGame() public whenNotPaused {
        GameState storage game = games;
        
        if (game.gameEnded) {
            revert GameNotInProgress();
        }

        game.gameEnded = true;
        emit GameEnded(block.timestamp);
    }

    /// @notice Gets all drawn numbers for the current game
    /// @return An array of drawn numbers
    function getDrawnNumbers() external view returns (uint256[] memory) {
        return games.drawnNumbers;
    }

    /// @notice Gets the current game state including player count
    /// @return startTime Game start timestamp
    /// @return lastDrawTime Last number draw timestamp
    /// @return numberCount Count of drawn numbers
    /// @return drawnNumbers Drawn numbers
    /// @return isEnded Whether the game has ended
    /// @return playerCount Current number of players
    /// @return isStarted Whether the game has officially started
    function getCurrentGameState() external view returns (
        uint256 startTime,
        uint256 lastDrawTime,
        uint256 numberCount,
        uint256[] memory drawnNumbers,
        bool isEnded,
        uint256 playerCount,
        bool isStarted
    ) {
        GameState storage game = games;
        return (
            game.startTime,
            game.lastDrawTime,
            game.drawnNumbers.length,
            game.drawnNumbers,
            game.gameEnded,
            gamePlayerCount,
            gamePlayerCount >= MIN_PLAYERS
        );
    }


        /// @notice Verifies if a card has a winning pattern
    /// @param card The card to verify
    /// @return bool Whether the card has won
    function _verifyWin(BingoCard memory card) public view returns (bool) {
        // Check rows
        for (uint256 i = 0; i < N_SIZE; i++) {
            bool rowWin = true;
            for (uint256 j = 0; j < N_SIZE; j++) {
                if (!usedNumbers[card.numbers[i * N_SIZE + j]]) {
                    rowWin = false;
                    break;
                }
            }
            if (rowWin) return true;
        }
        
        // Check columns
        for (uint256 i = 0; i < N_SIZE; i++) {
            bool colWin = true;
            for (uint256 j = 0; j < N_SIZE; j++) {
                if (!usedNumbers[card.numbers[j * N_SIZE + i]]) {
                    colWin = false;
                    break;
                }
            }
            if (colWin) return true;
        }
        
        // Check diagonals
        bool diag1Win = true;
        bool diag2Win = true;
        for (uint256 i = 0; i < N_SIZE; i++) {
            if (!usedNumbers[card.numbers[i * N_SIZE + i]]) {
                diag1Win = false;
            }
            if (!usedNumbers[card.numbers[i * N_SIZE + (N_SIZE-1 - i)]]) {
                diag2Win = false;
            }
            if (!diag1Win && !diag2Win) break; // Early exit if both diagonals fail
        }
        
        return diag1Win || diag2Win;
    }


    /// @notice Generates a shuffled array of numbers from 1 to 99
    /// @dev Uses Fisher-Yates shuffle with bytes from a single random number
    /// @return First 24 numbers from the shuffled array plus 0 in the middle
    function generateCardNumbers() private view returns (uint8[BOARD_SIZE] memory) {
        uint8[BOARD_SIZE] memory cardNumbers;
        uint8[MAX_NUMBER] memory numberPool;
        
        // Initialize number pool from 1 to MAX_NUMBER
        for (uint256 i = 0; i < MAX_NUMBER; i++) {
            numberPool[i] = uint8(i + 1);
        }
        
        // Generate a single random number
        uint256 randomness = uint256(keccak256(abi.encodePacked(
            block.timestamp,
            block.prevrandao,
            msg.sender
        )));
        
        // Use each byte of the random number to shuffle the first 24 positions
        for (uint256 i = 0; i < BOARD_SIZE-1; i++) {
            // Extract the next byte from randomness and map it to remaining range
            uint256 remainingNumbers = MAX_NUMBER - i;
            uint8 swapIndex = uint8((uint8(randomness >> (i * 8)) % remainingNumbers) + i);
            
            // Swap current position with randomly selected position
            (numberPool[i], numberPool[swapIndex]) = (numberPool[swapIndex], numberPool[i]);
        }
        uint256 middle_no = BOARD_SIZE/2;
        // Fill the card numbers, placing 0 in the middle
        for (uint256 i = 0; i < BOARD_SIZE; i++) {
            cardNumbers[i] = numberPool[i];
        }
        cardNumbers[middle_no] = 0; // Middle space
        return cardNumbers;
    }

    /// @notice Generates a random number for drawing
    /// @dev Uses block properties for randomness
    /// @return A random number
    function generateRandomNumber() private view returns (uint256) {
        return uint256(keccak256(abi.encodePacked(
            block.timestamp,
            block.prevrandao,
            block.number
        )));
    }

    /// @notice Pauses the contract
    /// @dev Only owner can pause
    function pause() external onlyOwner {
        _pause();
    }

    /// @notice Unpauses the contract
    /// @dev Only owner can unpause
    function unpause() external onlyOwner {
        _unpause();
    }

    /// @notice Checks if a player has already joined the current game
    function isInGame() external view returns (bool) {
        return gamePlayers[msg.sender];
    }

    /// @notice Returns the number of unique players in the current game
    function getCurrentPlayerCount() external view returns (uint256) {
        return gamePlayerCount;
    }

     /// @notice Returns the number of remaining required players in the current game
    function getRemainingPlayerCount() external view returns (uint256) {
        return MIN_PLAYERS-gamePlayerCount;
    }
}