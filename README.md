# Obiwan TFTP Server

[![stability-experimental](https://img.shields.io/badge/stability-experimental-orange.svg)](https://github.com/emersion/stability-badges#experimental)
![GitHub](https://img.shields.io/github/license/blitz/obiwan.svg)

Obiwan is a read-only TFTP server written in Haskell. At the moment,
it is a very barebones implementation of
[https://tools.ietf.org/html/rfc1350](TFTP (rev2)) without any bells
and whistles. Content is served from memory.

**Limitations:** This code is super experimental at the moment and only handles GET requests.

## Building

Obiwan is built using the Haskell Stack. Grab the
[https://docs.haskellstack.org/en/stable/install_and_upgrade/](stack) binary and
then building is as simple as:

```sh
% stack build
```

To build and run the tests:

```sh
% stack build --test
```

## Running

To run the example, run obiwan in one terminal:

```sh
$ stack exec obiwan
Listening on 127.0.0.1:12345
```

In another terminal, you can use a tftp client to fetch files:

```sh
% tftp -v 127.0.0.1 12345 -m binary -c get dmesg
mode set to octet
Connected to 127.0.0.1 (127.0.0.1), port 12345
getting from 127.0.0.1:dmesg to dmesg [octet]
Received 67837 bytes in 0.0 seconds [120117784 bit/s]
```
