<img src="site/static/package-logo.svg" width="128">

# Cargo local serve

Serve a local, offline, clone of `crates.io`.

DISCLAIMER: this is alpha software. Many features don't work yet
or are only prototyped.

Uses the crates you have cached locally to display a clone of the `crates.io` interface to users.

A second (and later) goal of this project is to extend with writeability, aka enabling a local crates.io like service for companies, mars colonists, etc where you can push your crates to, similar to how `crates.io` is working.

The `crates.io` team has publicly stated that the only goal of the codebase is to drive the site itself and no local clones of it. That's where this project comes in : it is tailored for that use case precisely. Aka, less setup but also only a subset of the features.

Some little demo for usage:

1. clone
2. do `cargo run` inside the repo
3. navigate your browser to some random crate, e.g. `http://localhost:3000/crate/winapi` or `http://localhost:3000/crate/futures`

## TODO

* Implement global scoring of crates by most depended on (directly), most depended on (transitive closure), IDK what else
* Obtain list of mirrored versions of a crate
* Add site for when the given version is not mirrored
* Implement "browse crates" page
* Upload feature

## Done

* Index page that displays a few "top" crates by various scorings
* Implement "versions/cratename" page
* Render the markdown using pulldown-cmark
* Source code formatting inside that rendering using syntect
* Search feature

## Design principles

The visual design has been heavily lended from the design
of the main `crates.io` website. It is a very beautiful design and well known to the Rust developer community.

The project is guided by the following principles:

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

## FAQ

### Do you want main crates.io to adopt your frontend?

I definitely encourage `crates.io` maintainers to take a look at my codebase and maybe draw inspiration for some improvements inside their own frontend. This is an open source project, everything is up for the grabs! But the main focus of this project is not to write a new frontend that covers the whole set of features that `crates.io` does.

### Do you think Ember is bad? If no, why didn't you use it then?

Ember is being used by many quite renown companies. If it were bad, they wouldn't be using it. In fact, this project itself is using the `handlebars` technology that has a deep relationship to ember.

I'm not the maintainer of `crates.io` so I don't want to tell them what to do. But for my project, I have chosen to not adopt ember or any other [single page applications](https://en.wikipedia.org/wiki/Single-page_application) (SPA) based approach.

The basic idea of designing web sites as single page applications (SPAs) is to *save* on bandwidth by just transporting a *small* json file over the wire. However, for crates I've tested, the json file that ember-powered `crates.io` sends over the network is *larger* than the entire HTML that my frontend sends over the wire. The second thing that SPA is about is having dynamic pages with content that changes over time either from itself (watching a live chat) or through interaction with the user (participating in a live chat). Now this project is about a mostly static site, so we'd get little gain here as well.

The third reason is a more security oriented one: if your entire interface with the application state is via json, it can be reasoned about by security tools in a much easier fashion. Also, any exploit in the rendering code would not allow you to escalate to a breach.

The first point is covered here by using handlebars: as handlebars uses json, there is already a json based interface. It is just inside the server! The second point is being approached by using memory-safe Rust everywhere. This obviously doesn't help with exploitable libraries, however they should mostly consist of safe Rust as well.

`crates.io` would probably see huge improvements in performance by using [ember-fastboot](https://ember-fastboot.com/). However, this project targets to be even leaner than that.

### Why did you choose the Iron framework?

I'm in fact a big fan of the [rocket](https://rocket.rs/) framework:

* Rocket makes stuff like URL parsing much easier than iron thanks to its macros. In general, the boilerplate is much less.
* Rocket is monolithic. With decentralized frameworks like iron, the documentation is split up, you never know whether crate A is compatible with crate B. This issue only gets worse when you have breaking changes involved: which version of crate A is meant to be used together with which version of crate B? Is crate B even deprecated? Sure, the idea to have crates focussed on small tasks is really great, but when the crates become *too* small, I think it is better to offer a monolithic API to users. You can still have optional features to turn off various components.
* In general, Rocket has much better docs than iron.

I would really love to use Rocket, but this service isn't just meant to be compiled once by some server inside the cloud (an environment very much under my control).
It is meant to be installed by a wide audience, on local machines, etc. Therefore, I don't want to require nightly, and I can't use Rocket as a consequence.

The [iron](https://github.com/iron/iron) web framework is mature and well established, compared to e.g. [gotham](https://gotham.rs/). That is why I chose it!

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
