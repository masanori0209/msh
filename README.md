# msh
Self Made Shell using Rust

# start

docker compose up -d

# build

docker compose exec msh cargo build

# release build

docker compose exec msh cargo build --release

# exec

docker compose exec msh /target/debug/msh
