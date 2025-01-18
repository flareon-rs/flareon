<div align="center">
<h1>Cot</h1>

[![Rust Build Status](https://github.com/cot-rs/cot/workflows/Rust%20CI/badge.svg)](https://github.com/cot-rs/cot/actions/workflows/rust.yml)
[![crates.io](https://img.shields.io/crates/v/cot.svg)](https://crates.io/crates/cot)
[![Documentation](https://docs.rs/cot/badge.svg)](https://docs.rs/cot)
[![codecov](https://codecov.io/gh/cot-rs/cot/branch/master/graph/badge.svg)](https://codecov.io/gh/cot-rs/cot)
[![Discord chat](https://img.shields.io/discord/1330137289287925781?logo=Discord&logoColor=white)](https://discord.cot.rs)
</div>

Cot is an easy to use, modern, and fast web framework for Rust. It has been designed to be familiar if you've ever
used [Django](https://www.djangoproject.com/), and easy to learn if you haven't. It's a batteries-included framework
built on top of [axum](https://github.com/tokio-rs/axum).

## Features

* **Easy to use API** — in many ways modeled after Django, Cot's API is designed to be easy to use and intuitive.
  Sensible defaults make it for easy rapid development, while the API is still empowering you when needed. The
  documentation is a first-class citizen in Cot, making it easy to find what you're looking for.
* **ORM integration** — Cot comes with its own ORM, allowing you to interact with your database in a way that feels
  Rusty and intuitive. Rust types are the source of truth, and the ORM takes care of translating them to and from the
  database, as well as creating the migrations automatically.
* **Type safe** — wherever possible, Cot uses Rust's type system to prevent common mistakes and bugs. Not only views
  are taking advantage of the Rust's type system, but also the ORM, the admin panel, and even the templates. All that to
  catch errors as early as possible.
* **Admin panel** — Cot comes with an admin panel out of the box, allowing you to manage your app's data with ease.
  Adding new models to the admin panel is stupidly simple, making it a great tool not only for rapid development and
  debugging, but with its customization options, also for production use.
* **Secure by default** — security should be opt-out, not opt-in. Cot takes care of making your web apps secure by
  default, defending it against common modern web vulnerabilities. You can focus on building your app, not securing it.

## Development

### Testing

Tests that require using external databases are ignored by default. In order to run them, execute the following in the
root of the repository:

```shell
docker compose up -d
cargo test --all-features -- --include-ignored
```

You can them execute the following command to stop the database:

```shell
docker compose down
```

## License

Cot is licensed under either of the following, at your option:

* Apache License, Version 2.0, ([LICENSE-APACHE](LICENSE-APACHE) or https://www.apache.org/licenses/LICENSE-2.0)
* MIT License ([LICENSE-MIT](LICENSE-MIT) or https://opensource.org/licenses/MIT)

Unless you explicitly state otherwise, any contribution intentionally submitted for inclusion in Cot by you shall be
dual licensed under the MIT License and Apache License, Version 2.0, without any additional terms or conditions.
