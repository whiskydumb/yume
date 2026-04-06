default:
    @just --list

dev:
    cargo run

build:
    cargo build --release

check:
    cargo clippy -- -D warnings
    cargo fmt -- --check

fmt:
    cargo fmt

test:
    cargo test

migrate:
    sqlx migrate run

db-reset:
    sqlx database drop -y
    sqlx database create
    sqlx migrate run

hash-password pw:
    cargo run --bin hash_password -- {{pw}}

prepare:
    cargo sqlx prepare
    
clean:
    cargo clean