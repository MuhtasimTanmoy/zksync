CREATE TABLE mint_nft_updates
(
    token_id           INT   PRIMARY KEY,
    block_number       BIGINT  NOT NULL,
    creator_account_id INT   NOT NULL,
    update_order_id    INT   NOT NULL,
    serial_id          INT   NOT NULL,
    address            bytea NOT NULL,
    content_hash       bytea NOT NULL,
    symbol             text  NOT NULL
);

CREATE TABLE nft
(
    token_id           INT   PRIMARY KEY,
    creator_account_id INT   NOT NULL,
    serial_id          INT   NOT NULL,
    address            bytea NOT NULL,
    content_hash       bytea NOT NULL
);