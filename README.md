# Aidoku Community Sources

This repository hosts unofficial sources maintained by community members that are installable in [Aidoku](https://github.com/Aidoku/Aidoku).

## ⚙️ Setup (primeira vez)

Instale as ferramentas necessárias:

```bash
# 1. Instala o target WebAssembly do Rust
rustup target add wasm32-unknown-unknown

# 2. Instala o CLI do Aidoku
cargo install --git https://github.com/Aidoku/aidoku-rs.git aidoku-cli
```

## 🚀 Como publicar as sources (após adicionar/alterar uma source)

Execute os comandos abaixo a partir da raiz do repositório:

```bash
# 1. Compila e empacota todas as sources
for src in sources/*/; do
  (cd "$src" && aidoku package)
done

# 2. Gera o index.min.json (source list)
aidoku build sources/*/package.aix --name "Arilton Sources"

# 3. Commita e envia para o GitHub
git add public/index.json public/index.min.json public/icons/ public/sources/
git commit -m "chore: deploy source list"
git push
```

Após o push, o arquivo estará disponível em:

```
https://ariltonjaguilar.github.io/sources/index.min.json
```

Para adicionar no Aidoku: **Settings → Source Lists → +** e cole a URL acima.

## 📦 Como adicionar uma nova source

1. Crie uma pasta em `sources/` com o padrão `lang.nomesource` (ex: `en.hentainexus`)
2. Adicione os arquivos:
   - `.cargo/config.toml` — configura o target wasm
   - `Cargo.toml` — dependências Rust
   - `src/lib.rs` — lógica da source
   - `res/source.json` — metadados (id, name, version, url, languages)
   - `res/icon.png` — ícone (PNG quadrado, ~144x144)
3. Teste compilando: `cd sources/sua.source && cargo build`
4. Execute os comandos de publicação acima

---

## Usage (original)

On a device with Aidoku (0.7+) installed, navigate to the settings tab, and under the source lists page add `https://ariltonjaguilar.github.io/sources/index.min.json`.

If a source is not working, or you want to request a source that isn't available in this source list, feel free to [create a new issue](https://github.com/Aidoku-Community/sources/issues).

## Contributing

Contributions are welcome! If you're new to Aidoku source development, check out [the official source development guide](https://aidoku.github.io/aidoku-rs/book/). Then, see [CONTRIBUTING.md](./CONTRIBUTING.md) for ways to contribute to this repo.

## License

This repo is licensed under either of Apache License, version 2.0 or MIT license at your option.

Unless you explicitly state otherwise, any contribution intentionally submitted for inclusion in this repository by you, as defined in the Apache-2.0 license, shall be dual licensed as above, without any additional terms or conditions.

## Disclaimer

This project does not have any affiliation with the content providers nor the Aidoku application.

If you own either a content provider or content that is hosted on one of the content providers that a source is offered for and wish not to have the source be made available on this repo, please contact us or [create a new issue](https://github.com/Aidoku-Community/sources/issues/new) to let us know and we will remove it.
