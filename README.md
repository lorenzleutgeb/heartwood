# ❤️🪵

*Radicle Heartwood Protocol & Stack*

Heartwood is the third iteration of the Radicle Protocol, a powerful
peer-to-peer code collaboration and publishing stack. The repository contains a
full implementation of Heartwood, complete with a user-friendly command-line
interface (`rad`) and network daemon (`radicle-node`).

Radicle was designed to be a secure, decentralized and powerful alternative to
code forges such as GitHub and GitLab that preserves user sovereignty
and freedom.

See the [Protocol Guide](https://radicle.xyz/guides/protocol) for an
in-depth description of how Radicle works.

## Installation

**Requirements**

* *Linux* or *Unix* based operating system.
* Git 2.34 or later
* OpenSSH 9.1 or later with `ssh-agent`

### 📀 From binaries

> Requires `curl` and `tar`.

Run the following command to install the latest binary release:

    curl -sSf https://radicle.xyz/install | sh

Or visit our [download](https://radicle.xyz/download) page.

### 📦 From source

> Requires the Rust toolchain.

You can install the Radicle stack from source, by running the following
commands from inside this repository:

    cargo install --path radicle-cli --force --locked --root ~/.radicle
    cargo install --path radicle-node --force --locked --root ~/.radicle
    cargo install --path radicle-remote-helper --force --locked --root ~/.radicle

Or directly from our seed node:

    cargo install --force --locked --root ~/.radicle \
        --git https://seed.radicle.xyz/z3gqcJUoA1n9HaHKufZs5FCSGazv5.git \
        radicle-cli radicle-node radicle-remote-helper

## Running

*Systemd* unit files are provided for the node under the `/systemd` folder.
They can be used as a starting point for further customization.

For running in debug mode, see [HACKING.md](HACKING.md).

## Feedback

If you have feedback, feel free to create issues using `rad issue`, join
[our Zulip][zulip], or email [feedback@radicle.xyz][mail-feedback].
Emails sent to this address are [automatically posted][zulip-help-email] to
[our **public** #feedback channel on Zulip][zulip-feedback], revealing the
[`From` header][rfc2822s3.6.2] (which usually contains your name and email
address). This allows us to discuss your feedback on Zulip, and, if necessary,
respond to you via email.

## Contributing

See [CONTRIBUTING.md](CONTRIBUTING.md) and [HACKING.md](HACKING.md) for an
introduction to contributing to Radicle.

## License

Radicle is distributed under the terms of both the MIT license and the Apache License (Version 2.0).

See [LICENSE-APACHE](LICENSE-APACHE) and [LICENSE-MIT](LICENSE-MIT) for details.

[zulip]: https://radicle.zulipchat.com/
[zulip-feedback]: https://radicle.zulipchat.com/#narrow/channel/392584-feedback
[zulip-help-email]: https://talently.zulip.com/help/message-a-channel-by-email
[mail-feedback]: mailto:feedback@radicle.xyz
[rfc2822s3.6.2]: https://datatracker.ietf.org/doc/html/rfc2822#section-3.6.2
