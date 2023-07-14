# Obiwan - TFTP Server for PXE Boot

[![stability-experimental](https://img.shields.io/badge/stability-experimental-orange.svg)](https://github.com/emersion/stability-badges#experimental)
![GitHub](https://img.shields.io/github/license/blitz/obiwan.svg)

## Introduction 🚀

Obiwan is a state-of-the-art TFTP server engineered specifically for
PXE Boot environments. It is designed to serve as a modern and secure
replacement for legacy TFTP server implementations written in C. With
a focus on security, performance, and simplicity, Obiwan integrates
the powerful and memory-safe Rust language with the high-performance
asynchronous capabilities of the Tokio library.

## Features 🌟

- **Read-Only**: Obiwan's mantra is safety. Tailored for PXE boot
  environments, it exclusively supports reading files to eliminate
  potential security loopholes and misconfigurations.

- **Security-First**: Obiwan takes advantage of Rust's memory safety and its minimalist design to substantially shrink the attack surface

- **OK Performance**: While staying simple, leveraging Tokio's
  asynchronous capabilities, Obiwan handles a plethora of concurrent
  file requests effortlessly.

- **No Configuration**: With sensible defaults, you just point it at a
  directory and off you go.

- **Free Software**: Obiwan thrives with your support and is open for
  contributions!

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

TODO

# Support

Should you encounter any issues or have questions, please open an issue on GitHub.
