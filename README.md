<img src="site/static/package-logo.svg" width="128">

# Cargo local serve

Serve a local, offline, clone of `crates.io`.

DISCLAIMER: this is alpha software. Many features don't work yet
or are only prototyped.

Uses the crates you have cached locally to display a clone of `crates.io`.

## TODO

* Implement global scoring of crates by most depended on (directly), most depended on (transitive closure), IDK what else
* Index page using that global scoring
* Obtain list of mirrored versions of a crate
* Add site for when the given version is not mirrored
* Implement  "crate/cratename/versions" page
* Implement "browse crates" page
* Source code formatting inside that rendering using syntect
* Search feature
* Upload feature

## Done

* Render the markdown using pulldown-cmark

## Design principles

The visual design has been heavily lended from the design
of the main `crates.io` website.

One of the design goals is however to be much leaner and
less wasteful about resources than `crates.io`.
The project is therefore guided by the following principles:

1. Any site shoud load fast and be low on resources.
	Any argument of the form "but nobody uses dialup any more" is not legitimate:
	Instead of being an opportunity for developers to waste stuff, a fast connection should make sites load faster!
2. While the site may feature Javascript based enhancements,
	if a feature can be implemented without Javascript, it should be.
	The site's main functionality should work even if no javascript
	is available.
3. In order to have the best developer and setup experience,
	no npm or node should be required in developing or running the site.
	Additionally, anything that calls itself a Javascript "framework" is disallowed (except for VanillaJs :p),
	as these require additional learning and lock you in.
	Any Javascript dependencies should be included manually and be small and lean,
	but generally Javascript dependencies should be avoided in order to be lean!
4. Only stable Rust may be used so that the service can be installed and used by a wide audience.
5. Usage of vendor prefixed features is forbidden. If a feature has been
	published as a Recommendation/Standard since, addition of vendor prefixes
	for it is tolerated if there is no other way to bring the feature
	to a targeted browser without Javascript.
6. The site should work without any internet available.
	Even if internet is available, there should be no requests by the frontend
	to any domain but the one the site lives on.
	Privacy invading malware like Google analytics is disallowed!

## Logo credit

The logo, under `site/static/package-logo.svg` has been adapted from
[a MIT licensed GitHub artwork](https://www.iconfinder.com/icons/298837/package_icon#size=128).

## License

Licensed under either of

* Apache License, Version 2.0, ([LICENSE-APACHE](LICENSE-APACHE) or http://www.apache.org/licenses/LICENSE-2.0)
* MIT license ([LICENSE-MIT](LICENSE-MIT) or http://opensource.org/licenses/MIT)

at your option.

### Contributions

Unless you explicitly state otherwise, any contribution intentionally submitted for inclusion in the work by you, as defined in the Apache-2.0 license, shall be dual licensed as above, without any additional terms or conditions.
