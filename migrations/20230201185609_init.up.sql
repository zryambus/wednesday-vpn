-- Add up migration script here

CREATE TABLE IF NOT EXISTS profiles (
    name TEXT NOT NULL,
    user_id BIGINT NOT NULL,
    ip INET NOT NULL,
    private_key TEXT NOT NULL,
    public_key TEXT NOT NULL,
    only_local BOOLEAN NOT NULL
);

CREATE TABLE IF NOT EXISTS invites (
    id UUID NOT NULL
);

CREATE TYPE user_status AS ENUM ('none', 'requested', 'granted', 'restricted');

CREATE TABLE IF NOT EXISTS users (
    user_id BIGINT NOT NULL,
    status user_status DEFAULT 'none'
);