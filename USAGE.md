# Cargo local serve usage notes

## Managing crate storage

You need a local copy of all crates.io crates
in order to use cargo-local-serve.
It ships with facilities to obtain such a copy
from crates.io itself.

### Downloading the crates (ArchiveTree)

In order to download all crates that
are stored in the local registry,
just run the command:
```
cargo run --release -p all-crates-storage --bin download-all-crates
```
It will download all crates to the `crate-archives/`
directory.

This storage method is referred to as
`ArchiveTree` in config.toml.

### Compressing all crates (StorageFile)

You can create a compressed file
from already locally present .crate files
using this command:
```
cargo run --release -p all-crates-storage --bin create-crate-storage
```

This storage method is referred to as
`StorageFile` in config.toml.

### Updating

The downloader to `ArchiveTree` storage
is smart and only downloads things
that are not present on disk.

Right now, the `StorageFile` compressor
is not really smart. It doesn't update
already present files.
However, it should be easy to add such
a mode if desired. Please contact the
author or try to make a patch yourself.

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
This can be done by using the replacement mechanism,
setable in `.cargo/config`.

```toml
[source.cargo-local-serve]
registry = "file:///path/to/src/crates.io-index/clone"

[source.crates-io]
replace-with = "cargo-local-serve"
```
