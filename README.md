# (Blockchain) Explorer

A no-frills explorer focused on speed and providing in-depth information


## Get Started

### 1. Setup local dependency

For the time being there's a local dependency that must be manually placed before proceeding with the regular setup.

To satisfy this dependency you need to clone [be-cash/bitcoin-cash](https://github.com/be-cash/bitcoin-cash) on the same level as this repository. If you check [explorer-server/Cargo.toml](explorer-server/Cargo.toml) you'll see that Cargo expects this folder to be two levels up.

```toml
[dependencies]
bitcoin-cash = { path = "../../bitcoin-cash/bitcoin-cash" }
```

Your folder structure should look similar to this:

```
bitcoin-cash
├── bitcoin-cash
├── bitcoin-cash-*
├── Cargo.lock
├── Cargo.toml
├── LICENSE
└── rustfmt.toml
explorer
├── Cargo.lock
├── Cargo.toml
└── explorer-server
```

### 2. Configure & Run

```
cd explorer-server/
cp config.dist.toml config.toml
```

The ([config.dist.toml](explorer-server/config.dist.toml)) is just an example on which to base your config from:

```toml
mode = "development"
index_database = "../index.test.rocksdb"
host = "0.0.0.0:3035"
```

You're all done! Now you can run the project:
```
cargo run
```

Go to http://localhost:3035 and you should see the homepage

## Supported Chains

- [x] eCash XEC
- [ ] Lotus XPI (coming soon)
