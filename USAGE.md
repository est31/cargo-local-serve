# Cargo local serve usage notes

## config.toml

cargo local serve itself is configurable.
It per default looks for a `config.toml`
file in its `PATH`.
You can copy `config.toml.example`
and modify it according to your needs.

## Making cargo point at it

One of the use cases that cargo local serve
was built for is the scenario where you lack
internet access yet wish to develop Rust
applications.

cargo local serve offers a crates.io like API
that cargo understands.
The following steps let you make cargo point at
cargo local serve:

First, you'll need a clone of the crates.io index.
You can obtain one by doing:

```
git clone https://github.com/rust-lang/crates.io-index
```

Then edit the `config.json` file it contains to read:

```json
{
  "dl": "http://localhost:3000/api/v1/crates",
  "api": "http://localhost:3000/"
}
```

Or to whatever your host/port combination
cargo local serve is listening on.

Now you only have to tell cargo to use that index.
This can be done by using the replacement mechanism.

```toml
[source.cargo-local-serve]
registry = "file:///path/to/src/crates.io-index/clone"

[source.crates-io]
replace-with = "cargo-local-serve"
```
