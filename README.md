# Obiwan - TFTP Server for PXE Boot

[![stability-experimental](https://img.shields.io/badge/stability-experimental-orange.svg)](https://github.com/emersion/stability-badges#experimental)
![GitHub](https://img.shields.io/github/license/blitz/obiwan.svg)

## Introduction 🚀

Obiwan is a TFTP server engineered specifically for PXE Boot
environments. It is designed to serve as a modern and secure
replacement for legacy TFTP server implementations written in C. With
a focus on security, performance, and simplicity, Obiwan integrates
the powerful and memory-safe Rust language with the high-performance
asynchronous capabilities of the Tokio library.

## Features 🌟

- **Read-Only**: Obiwan's mantra is safety. Tailored for PXE boot
  environments, it exclusively supports reading files to eliminate
  potential security loopholes and misconfigurations.

- **Security-First**: Obiwan takes advantage of Rust's memory safety
  and its minimalist design to substantially shrink the attack
  surface.

- **OK Performance**: While staying simple, leveraging Tokio's
  asynchronous capabilities, Obiwan handles a plethora of concurrent
  file requests effortlessly.

- **No Configuration**: With sensible defaults, you just point it at a
  directory and off you go.

- **Free Software**: Obiwan thrives with your support and is open for
  contributions!

## Tested Clients

These clients are checked via CI:

- [atftp](https://sourceforge.net/projects/atftp/)
- [tftp-hpa / in.tftp](https://mirrors.edge.kernel.org/pub/software/network/tftp/tftp-hpa/)

The following clients have been reported to work:

- Lenovo ThinkStation P360 UEFI
- [iPXE](https://ipxe.org/)

Feel free to open a PR to add to these lists!

## Contributing

Obiwan is currently experimental and is missing features and
testing. Most welcome are contributions that improve documentation,
increase test coverage, or implement missing TFTP extensions. Security
improvements, such as reducing the number of dependencies or improving
sandboxing are also highly welcome. Performance improvements, such as
removing memory allocations, are also welcome as long as they don't
complicate the code base.

Obiwan will never support writing files. Please do not try to add this
feature.

## Getting Started 🏁

### NixOS

This documentation assumes that your [NixOS](https://nixos.org/)
system is built as a [Nix Flake](https://nixos.wiki/wiki/Flakes).

In your `flake.nix`, add Obiwan as an input and enable the module in
a NixOS configuration:

```nix
{
  # ...

  inputs = {
    # ... other inputs ...

    obiwan = {
      url = "github:blitz/obiwan";

      # Optional to reduce the system closure. May not work
      # inputs.nixpkgs.follows = "nixpkgs";
    };
  };

  outputs = { self, nixpkgs, obiwan ... }: {
    nixosConfigurations.machine = nixpkgs.lib.nixosSystem {
      system = "x86_64-linux";
      modules = [
        # ... other modules ...

        obiwan.nixosModules.default

        ./machine.nix
      ];
    };
  };
}
```

You can then enable Obiwan by adding the following configuration in
`machine.nix`:

```nix
{ config, pkgs, lib, ... }: {
  # ... other configuration ...

  services.obiwan = {
    enable = true;

    # The directory that will be made available via TFTP. Must exist or the
    # service will fail to start.
    root = "/srv/tftp";

    # The IP the service will listen on.
    listenAddress = "192.168.1.1";
  };
}
```

Check `nix/module.nix` in this repository for other configuration
options.

### Other Linux

Obiwan is a Rust application without special dependencies. With a
recent Rust toolchain, you can build and install it with `cargo`:

```console
$ cd ws/obiwan

# Check that all unit tests pass.
$ cargo test

# Build the release version.
$ cargo build --release

# Install it into $HOME/.cargo/bin
$ cargo install --path .
```

To run Obiwan as a systemd unit, you can take inspiration from
`nix/module.nix`. See `systemd.services.obiwan` for the NixOS systemd
unit description, which should be a good starting point for any other
Linux.

# Support

Should you encounter any issues or have questions, please open an
issue on GitHub.
