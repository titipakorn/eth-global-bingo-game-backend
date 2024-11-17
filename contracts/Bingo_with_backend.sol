// SPDX-License-Identifier: MIT
pragma solidity ^0.8.19;

import "@openzeppelin/contracts/security/Pausable.sol";
import "@openzeppelin/contracts/access/Ownable.sol";

/// @title Simplified Bingo Game Smart Contract with Off-chain Random Seed
/// @notice Implements a decentralized Bingo game with backend-generated random seed
contract BingoGame is Pausable, Ownable {

    // Custom errors
    error GameAlreadyInProgress();
    error GameNotInProgress();
    error InvalidCardPurchase();
    error InvalidDrawInterval();
    error InsufficientPlayers();
    error UnauthorizedOperator();
    error InvalidWin();
    error CardAlreadyAssigned();

    // Constants
    uint256 public MIN_PLAYERS = 2;
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
    address[] private players;
    mapping(address => bool) private gamePlayers;
    uint256 private gamePlayerCount;
    mapping(uint256 => bool) private usedNumbers;
    address public operator;
    
    // Events
    event GameStarted(uint256 timestamp);
    event CardPurchased(address indexed player);
    event NumberDrawn(uint256 number);
    event WinClaimed(address indexed player);
    event GameEnded(uint256 timestamp);
    event OperatorUpdated(address newOperator);

    constructor(address _operator) Ownable(msg.sender) {
        operator = _operator;
        emit OperatorUpdated(_operator);
    }

    function updateOperator(address newOperator) external onlyOwner {
        operator = newOperator;
        emit OperatorUpdated(newOperator);
    }

    modifier onlyOperator() {
        if (msg.sender != operator) {
            revert UnauthorizedOperator();
        }
        _;
    }

    function _startNewGame() private {
        GameState storage newGame = games;
        newGame.startTime = block.timestamp;
        newGame.lastDrawTime = block.timestamp;
        newGame.gameEnded = false;
        emit GameStarted(block.timestamp);
    }

    function _resetUsedNumbers() private {
        for(uint256 i = 1; i <= MAX_NUMBER; i++) {
            usedNumbers[i] = false;
        }
    }

    function _resetCards() private {
        for(uint256 i = 1; i <= players.length; i++) {
            delete cards[players[i]];
            gamePlayers[players[i]] = false;
        }
    }

    /// @notice Generates card numbers from a random seed
    /// @param randomSeed The random seed from backend
    /// @return cardNumbers Generated card numbers
    function generateCardNumbers(uint256 randomSeed) private pure returns (uint8[BOARD_SIZE] memory) {
        uint8[BOARD_SIZE] memory cardNumbers;
        uint8[MAX_NUMBER] memory numberPool;
        
        // Initialize number pool from 1 to MAX_NUMBER
        for (uint256 i = 0; i < MAX_NUMBER; i++) {
            numberPool[i] = uint8(i + 1);
        }
        
        // Use each byte of the random seed to shuffle the first 24 positions
        for (uint256 i = 0; i < BOARD_SIZE-1; i++) {
            
            uint8 swapIndex = uint8((uint8(randomSeed >> (i * 8)) % MAX_NUMBER) + i);
            
            // Swap current position with randomly selected position
            (numberPool[i], numberPool[swapIndex]) = (numberPool[swapIndex], numberPool[i]);
        }
    
        // Fill the card numbers, placing 0 in the middle
        for (uint256 i = 0; i < BOARD_SIZE; i++) {
            cardNumbers[i] = numberPool[i];
        }
        cardNumbers[BOARD_SIZE/2] = 0; // Middle space
        return cardNumbers;
    }

    /// @notice Assigns a card to a player using a backend-provided random seed
    /// @param player Player address to assign the card to
    /// @param randomSeed Random seed from backend
    function assignCard(
        address player,
        uint256 randomSeed
    ) external whenNotPaused onlyOperator {
        if (games.startTime > 0) {
            revert GameAlreadyInProgress();
        }
        if (cards[player].isInit) {
            revert CardAlreadyAssigned();
        }

        uint8[BOARD_SIZE] memory cardNumbers = generateCardNumbers(randomSeed);
        players.push(player);
        cards[player] = BingoCard({
            owner: player,
            numbers: cardNumbers,
            hasWon: false,
            isInit: true
        });

        if (!gamePlayers[player]) {
            gamePlayers[player] = true;
            gamePlayerCount++;

            if (gamePlayerCount == MIN_PLAYERS) {
                _startNewGame();
            }
        }

        emit CardPurchased(player);
    }

    function submitDrawnNumber(uint256 number) external whenNotPaused onlyOperator {
        GameState storage game = games;
        
        if (game.gameEnded) {
            revert GameNotInProgress();
        }
        if (gamePlayerCount < MIN_PLAYERS) {
            revert InsufficientPlayers();
        }
        if (number > MAX_NUMBER || number == 0) {
            revert InvalidDrawInterval();
        }
        if (usedNumbers[number]) {
            revert InvalidDrawInterval();
        }

        game.drawnNumbers.push(number);
        usedNumbers[number] = true;
        game.lastDrawTime = block.timestamp;

        emit NumberDrawn(number);

        if (game.drawnNumbers.length >= MAX_NUMBER) {
            _endGame();
        }
    }

    function getPlayerCards(address player) external view returns (uint8[25] memory storedNumbers) {
        require(cards[player].isInit, "No card!");
        storedNumbers = cards[player].numbers;
        return storedNumbers;
    }

    function claimWin(address player) external whenNotPaused returns (bool) {
        BingoCard storage card = cards[player];

        if (card.hasWon) {
            revert InvalidWin();
        }

        if (!_verifyWin(card)) {
            revert InvalidWin();
        }
        card.hasWon = true;
        emit WinClaimed(player);
        _endGame();
        return true;
    }

    function _endGame() private whenNotPaused {
        GameState storage game = games;
        
        if (game.gameEnded) {
            revert GameNotInProgress();
        }
        _resetCards();
        gamePlayerCount=0;
        _resetUsedNumbers();
        game.gameEnded = true;
        game.drawnNumbers = new uint256[](1);
        game.drawnNumbers[0] = 0;
        usedNumbers[0] = true;
        emit GameEnded(block.timestamp);
    }

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
            if (!diag1Win && !diag2Win) break;
        }
        
        return diag1Win || diag2Win;
    }

    // View functions remain the same
    function getDrawnNumbers() external view returns (uint256[] memory) {
        return games.drawnNumbers;
    }

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

    function pause() external onlyOwner {
        _pause();
    }

    function unpause() external onlyOwner {
        _unpause();
    }

    function isInGame(address player) external view returns (bool) {
        return gamePlayers[player];
    }

    function isGameStarted() external view returns (bool) {
        return games.startTime>0;
    }

    function getCurrentPlayerCount() external view returns (uint256) {
        return gamePlayerCount;
    }

    function getRemainingPlayerCount() external view returns (uint256) {
        return MIN_PLAYERS-gamePlayerCount;
    }
}