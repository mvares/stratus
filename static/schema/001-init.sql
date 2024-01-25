CREATE TABLE IF NOT EXISTS accounts (
    address BYTEA NOT NULL CHECK (LENGTH(address) = 20) UNIQUE,
    nonce NUMERIC NOT NULL CHECK (nonce >= 0),
    balance NUMERIC NOT NULL CHECK (balance >= 0),
    bytecode BYTEA CHECK (LENGTH(bytecode) <= 24000),
    block_number BIGSERIAL NOT NULL CHECK (block_number >= 0) UNIQUE,
    PRIMARY KEY (address, block_number)
);

CREATE TABLE IF NOT EXISTS account_slots (
    idx BYTEA NOT NULL CHECK (LENGTH(idx) = 32),
    value BYTEA NOT NULL CHECK (LENGTH(value) = 32),
    account_address BYTEA NOT NULL REFERENCES accounts (address),
    block_number BIGSERIAL NOT NULL CHECK (block_number >= 0),
    PRIMARY KEY (idx, account_address, block_number)
);

-- TODO: maybe call this table `block_headers`
CREATE TABLE IF NOT EXISTS blocks (
    number BIGSERIAL NOT NULL CHECK (number >= 0) UNIQUE
    ,hash BYTEA NOT NULL CHECK (LENGTH(hash) = 32) UNIQUE
    ,transactions_root BYTEA NOT NULL CHECK (LENGTH(transactions_root) = 32)
    ,gas NUMERIC NOT NULL CHECK (gas >= 0)
    ,logs_bloom BYTEA NOT NULL CHECK (LENGTH(logs_bloom) = 256)
    ,timestamp_in_secs INTEGER NOT NULL CHECK (timestamp_in_secs >= 0)
    ,created_at TIMESTAMP NOT NULL
    ,PRIMARY KEY (number, hash)
);

CREATE TABLE IF NOT EXISTS transactions (
    hash BYTEA NOT NULL CHECK (LENGTH(hash) = 32) UNIQUE,
    signer_address BYTEA NOT NULL CHECK (LENGTH(signer_address) = 20),
    nonce NUMERIC NOT NULL CHECK (nonce >= 0),
    address_from BYTEA NOT NULL CHECK (LENGTH(address_from) = 20),
    address_to BYTEA CHECK (LENGTH(address_to) = 20),
    input BYTEA NOT NULL CHECK (LENGTH(input) <= 24000),
    gas NUMERIC NOT NULL CHECK (gas >= 0),
    idx_in_block SERIAL NOT NULL CHECK (idx_in_block >= 0) UNIQUE,
    block_number BIGSERIAL REFERENCES blocks(number) NOT NULL CHECK (block_number >= 0),
    block_hash BYTEA REFERENCES blocks(hash) NOT NULL CHECK (LENGTH(block_hash) = 32),
    PRIMARY KEY (hash)
);

CREATE TABLE IF NOT EXISTS logs (
    address BYTEA NOT NULL CHECK (LENGTH(address) = 20)
    ,data BYTEA NOT NULL
    ,transaction_hash BYTEA REFERENCES transactions(hash) NOT NULL CHECK (LENGTH(transaction_hash) = 32)
    ,transaction_idx SERIAL REFERENCES transactions(idx_in_block) NOT NULL CHECK (transaction_idx >= 0)
    ,log_idx SERIAL NOT NULL CHECK (log_idx >= 0) UNIQUE
    ,block_number BIGSERIAL REFERENCES blocks(number) NOT NULL CHECK (block_number >= 0)
    ,block_hash BYTEA REFERENCES blocks(hash) NOT NULL CHECK (LENGTH(block_hash) = 32)
);

CREATE TABLE IF NOT EXISTS topics (
    topic BYTEA NOT NULL CHECK (LENGTH(topic) = 32)
    ,transaction_hash BYTEA REFERENCES transactions(hash) NOT NULL CHECK (LENGTH(transaction_hash) = 32)
    ,transaction_idx SERIAL REFERENCES transactions(idx_in_block) NOT NULL CHECK (transaction_idx >= 0)
    ,log_idx SERIAL REFERENCES logs(log_idx) NOT NULL CHECK (log_idx >= 0)
    ,block_number BIGSERIAL REFERENCES blocks(number) NOT NULL CHECK (block_number >= 0)
    ,block_hash BYTEA REFERENCES blocks(hash) NOT NULL CHECK (LENGTH(block_hash) = 32)
    ,PRIMARY KEY (topic)
);

CREATE SEQUENCE IF NOT EXISTS block_number_seq
    AS BIGINT
    MINVALUE 0
    START WITH 0
    INCREMENT BY 1
    NO CYCLE
    OWNED BY blocks.number;