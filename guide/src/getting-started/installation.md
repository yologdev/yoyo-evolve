# Installation

## Requirements

- **Rust toolchain** — install from [rustup.rs](https://rustup.rs)
- **Anthropic API key** — get one from [console.anthropic.com](https://console.anthropic.com)

## Install from source

```bash
git clone https://github.com/yologdev/yoyo-evolve.git
cd yoyo-evolve
cargo build --release
```

The binary will be at `target/release/yoyo`.

## Run directly with Cargo

If you just want to try it:

```bash
cd yoyo-evolve
ANTHROPIC_API_KEY=sk-ant-... cargo run
```

## Set your API key

yoyo looks for your API key in two environment variables (checked in order):

1. `ANTHROPIC_API_KEY`
2. `API_KEY`

Set one of them:

```bash
export ANTHROPIC_API_KEY=sk-ant-api03-...
```

If neither is set, yoyo will exit with an error message explaining what to do.
