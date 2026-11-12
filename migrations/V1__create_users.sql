-- Phase 1 schema: users only. Must stay idempotent (re-run on every setup).
--
-- Application responsibilities (the DB cannot do these):
--   * *_normalized columns: apply Unicode NFKC + lowercase in the app BEFORE
--     insert/lookup. Uniqueness and login lookups use only these columns.
--   * password_hash: store a full argon2id PHC string (it carries its own
--     parameters, so you can rehash on login later). Never store plaintext.
--   * Wrap transactions in a retry loop for SQLSTATE 40001 (serialization).
--
-- user_id is a random UUID on purpose: in CockroachDB, sequential or
-- time-ordered keys concentrate inserts on one range.

CREATE TABLE users (
    user_id             UUID        NOT NULL DEFAULT gen_random_uuid(),
    username            STRING      NOT NULL,  -- display form, as typed
    username_normalized STRING      NOT NULL,
    email               STRING      NOT NULL,  -- display form, as typed
    email_normalized    STRING      NOT NULL,
    password_hash       STRING      NOT NULL,
    created_at          TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at          TIMESTAMPTZ NOT NULL DEFAULT now() ON UPDATE now(),

    CONSTRAINT users_pkey PRIMARY KEY (user_id),
    CONSTRAINT users_username_normalized_key UNIQUE (username_normalized),
    CONSTRAINT users_email_normalized_key UNIQUE (email_normalized),
    CONSTRAINT users_username_normalized_nonempty CHECK (username_normalized <> ''),
    CONSTRAINT users_email_normalized_nonempty CHECK (email_normalized <> ''),
    CONSTRAINT users_password_hash_nonempty CHECK (password_hash <> '')
);