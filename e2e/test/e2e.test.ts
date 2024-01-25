import { expect } from "chai";
import { keccak256 } from "ethers";
import { ethers } from "hardhat";
import { match } from "ts-pattern";
import { Block, Transaction, TransactionReceipt } from "web3-types";

import { TestContract } from "../typechain-types";
import { ALICE, BOB, CHARLIE } from "./helpers/account";
import { CURRENT_NETWORK, Network } from "./helpers/network";
import * as rpc from "./helpers/rpc";

// -----------------------------------------------------------------------------
// Constants
// -----------------------------------------------------------------------------

const CHAIN_ID_DEC = 2008;
const CHAIN_ID = rpc.toHex(CHAIN_ID_DEC);

// Special numbers
const ZERO = "0x0";
const ONE = "0x1";

// Special hashes
const HASH_EMPTY_UNCLES = "0x1dcc4de8dec75d7aab85b567b6ccd41ad312451b948a7413f0a142fd40d49347";
const HASH_EMPTY_TRANSACTIONS = "0x56e81f171bcc55a6ff8345e692c0f86e5b48e01b996cadc001622fb5e363b421";

// Test contract topics
const CONTRACT_TOPIC_ADD = "0x2728c9d3205d667bbc0eefdfeda366261b4d021949630c047f3e5834b30611ab";
const CONTRACT_TOPIC_SUB = "0xf9c652bcdb0eed6299c6a878897eb3af110dbb265833e7af75ad3d2c2f4a980c";

// -----------------------------------------------------------------------------
// RPC tests
// -----------------------------------------------------------------------------
describe("JSON-RPC", () => {
    describe("Metadata", () => {
        it("eth_chainId", async () => {
            (await rpc.sendExpect("eth_chainId")).eq(CHAIN_ID);
        });
        it("net_version", async () => {
            (await rpc.sendExpect("net_version")).eq(CHAIN_ID_DEC + "");
        });
        it("web3_clientVersion", async () => {
            let client = await rpc.sendExpect("web3_clientVersion");
            match(CURRENT_NETWORK).with(Network.Stratus, () => client.deep.eq("stratus"));
        });
    });

    describe("Gas", () => {
        it("eth_gasPrice", async () => {
            let gasPrice = await rpc.sendExpect("eth_gasPrice");
            match(CURRENT_NETWORK)
                .with(Network.Stratus, () => gasPrice.eq(ZERO))
                .otherwise(() => gasPrice.not.eq(ZERO));
        });
    });

    describe("Account", () => {
        it("eth_getTransactionCount", async () => {
            (await rpc.sendExpect("eth_getTransactionCount", [ALICE])).eq(ZERO);
            (await rpc.sendExpect("eth_getTransactionCount", [ALICE, "latest"])).eq(ZERO);
        });
    });

    describe("Block", () => {
        it("eth_blockNumber", async function () {
            (await rpc.sendExpect("eth_blockNumber")).eq(ZERO);
        });
        it("eth_getBlockByNumber", async function () {
            let block: Block = await rpc.send("eth_getBlockByNumber", [ZERO, true]);
            expect(block.transactions.length).eq(0);
        });
    });
});

// -----------------------------------------------------------------------------
// Native transfer tests
// -----------------------------------------------------------------------------
describe("Wei Transaction", () => {
    var _tx: Transaction;
    var _txHash: string;
    var _block: Block;

    it("Send transaction", async () => {
        let txSigned = await ALICE.signer().signTransaction({
            chainId: CHAIN_ID_DEC,
            to: BOB,
            value: 0,
            gasPrice: 0,
            gasLimit: 500_000,
        });
        _txHash = await rpc.send("eth_sendRawTransaction", [txSigned]);
        expect(_txHash).eq(keccak256(txSigned));
    });
    it("Transaction is created", async () => {
        _tx = await rpc.send("eth_getTransactionByHash", [_txHash]);
        expect(_tx.from).eq(ALICE.address, "tx.from");
        expect(_tx.to).eq(BOB.address, "tx.to");
        expect(_tx.nonce).eq(ZERO, "tx.nonce");
        expect(_tx.chainId).eq(CHAIN_ID, "tx.chainId");
    });
    it("Block is created", async () => {
        expect(await rpc.send("eth_blockNumber")).eq(ONE);

        _block = await rpc.send("eth_getBlockByNumber", [ONE, true]);
        expect(_block.number).eq(ONE);

        expect(_block.transactionsRoot).not.eq(HASH_EMPTY_TRANSACTIONS);
        expect(_block.uncles).lengthOf(0);
        expect(_block.sha3Uncles).eq(HASH_EMPTY_UNCLES);

        expect(_block.transactions.length).eq(1);
        expect(_block.transactions[0] as Transaction).deep.eq(_tx);
    });
    it("Receipt is created", async () => {
        let receipt: TransactionReceipt = await rpc.send("eth_getTransactionReceipt", [_txHash]);
        expect(receipt.blockNumber).eq(_block.number, "receipt.blockNumber");
        expect(receipt.blockHash).eq(_block.hash, "receipt.blockHash");
        expect(receipt.transactionHash).eq(_txHash, "rceipt.txHash");
        expect(receipt.transactionIndex).eq(ZERO, "receipt.txIndex");
        expect(receipt.from).eq(ALICE.address, "receipt.from");
        expect(receipt.status).eq(ONE, "receipt.status");
    });
    it("Sender nonce increased", async () => {
        expect(await rpc.send("eth_getTransactionCount", [ALICE, "latest"])).eq(ONE);
    });
    it("Receiver nonce not increased", async () => {
        expect(await rpc.send("eth_getTransactionCount", [BOB, "latest"])).eq(ZERO);
    });
});

// -----------------------------------------------------------------------------
// Contracts tests
// -----------------------------------------------------------------------------
describe("Contract", async () => {
    var _contract: TestContract;
    var _block: bigint;

    it("Is deployed", async () => {
        const testContractFactory = await ethers.getContractFactory("TestContract");
        _contract = await testContractFactory.connect(CHARLIE.signer()).deploy();
    });

    it("Performs add/bub", async () => {
        _block = await rpc.WEB3.eth.getBlockNumber();

        // initial balance
        expect(await _contract.get(CHARLIE.address)).eq(0);

        // mutations
        let nonce = await rpc.ETHERJS.getTransactionCount(CHARLIE.address);
        const ops = _contract.connect(CHARLIE.signer());
        await ops.add(10, { nonce: nonce++ });
        await ops.add(15, { nonce: nonce++ });
        await ops.sub(5, { nonce: nonce++ });

        // final balance
        expect(await _contract.get(CHARLIE.address)).eq(20);
    });

    it("Generates logs", async () => {
        let f = { address: _contract.target, fromBlock: rpc.toHex(_block + 0n) };

        // filter fromBlock
        (await rpc.sendExpect("eth_getLogs", [{ ...f, fromBlock: rpc.toHex(_block + 0n) }])).length(3);
        (await rpc.sendExpect("eth_getLogs", [{ ...f, fromBlock: rpc.toHex(_block + 1n) }])).length(3);
        (await rpc.sendExpect("eth_getLogs", [{ ...f, fromBlock: rpc.toHex(_block + 2n) }])).length(2);
        (await rpc.sendExpect("eth_getLogs", [{ ...f, fromBlock: rpc.toHex(_block + 3n) }])).length(1);
        (await rpc.sendExpect("eth_getLogs", [{ ...f, fromBlock: rpc.toHex(_block + 4n) }])).length(1);

        // filter topics
        (await rpc.sendExpect("eth_getLogs", [{ ...f, topics: [CONTRACT_TOPIC_ADD] }])).length(2);
        (await rpc.sendExpect("eth_getLogs", [{ ...f, topics: [CONTRACT_TOPIC_SUB] }])).length(1);
    });
});