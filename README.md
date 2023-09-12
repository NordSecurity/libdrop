# libdrop
libdrop is a library for sending/receiving files, primarily over meshnet, but
WAN is also an option.

# Releasing
- The version must be set in `LIBDROP_RELEASE_NAME` env variable before compiling the code. The value is embedded inside the binary as the release
version. The format is `v{semver}`. An API call to `norddrop_version()` should be made to ensure that.
- Before releasing `changelog.md` must be updated with the version name being released. Best if the tag is pointing to the commit updating `changelog.md`.
- After the release the `main` branch must be merged into the `dev` branch.
- After changes are introduced it might need an appsec review. This is especially true if the major semver component is increased.

# Tests
## Whole testsuite
```
make -C test
```

## Individual tests
Testsuite takes a long time to complete so running specific tests might be preferential and much faster while developing. To run a specific testsuite:
```
SCENARIO=scenario_name make -C test
```

## Code coverage single test
```
SCENARIO=scenario_name make -C test coverage
```

## Code coverage all testcases
```
make -C test coverage
```
## udrop

udrop is an example client-server to test basic functionality of the library.

## Build and run (server)
A container image can be built with the example binary ready for running:
```
run.py server run
```

## Run (client)
```
export DROP_SERVER=172.17.0.2
cargo run --example udrop -- -l 0.0.0.0 transfer $DROP_SERVER <path>
```

`<path>` is whatever file or folder you want to transfer to the server.

You can verify the transfer by checking the file system in the server container under `/root/<path>`

## Generating file ids from shell
```bash
echo -n "<absolute file path>" | sha256sum  | cut -d " " -f1 | xxd -ps -r | basenc --base64url | tr -d '='
```

## IPv6 format
Whenever IPv6 is used in a text form e.g. it is stored in the DB, included in the event, or passed as a callback argument we use the [RFC 5952](https://tools.ietf.org/html/rfc5952) for unique text representation.

# Contributing
[CONTRIBUTING.md](CONTRIBUTING.md)

# Licensing
[This project is licensed under the terms of the GNU General Public License v3.0 only](LICENSE)
