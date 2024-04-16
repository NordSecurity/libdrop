# libdrop
libdrop is a library for sending/receiving files, primarily over meshnet, but
WAN is also an option.

# Releases
## Technical
- The version must be set in `LIBDROP_RELEASE_NAME` env variable before compiling the code. The value is embedded inside the binary as the release
version. API call  `norddrop_version()` returns this string.

## Versions and Branches
Branches used for development:
- `dev` - development, faster moving branch. Moose tracker latest with context sharing.
- `main` - release branch, contains latest release. Moose tracker latest with context sharing.

Maintained release branches:
- `release/v5` - Maintenance, for Linux, Apple.
- `release/v6` - Pre UniFFI. Android, Windows.
- `release/v7` - UniFFI. Android, Windows.

As of 6.4.0, 5.5.0, 7.0.0 versions libdrop needs to maintain two versions of Moose tracker. One with
context sharing and the other with not. Both are not backwards compatible(in API and in between), thus Libdrop 
must maintain two release branches for a while.

Branches and releases must contain `moose-no-context` or `moose-with-context` in the name to
indicate which moose tracker is being used. *This should be a temporary nuisance, a constant re-evaluation is needed*.

# Release Procedure
- After the release the `main` branch must be merged into the `dev` branch.
- After changes are introduced it might need an appsec review. This is especially true if the major semver component is increased.

1. Check the `changelog.md` file and make appropriate missing changes if any. Enter new version in there at the top if not there.
Preferrably there should be no version before the release happens because it's easy to assume it's an already released version.
Prefer to have *UNRELEASED* as a placeholder.

2. Tag the commit with changelog modifications.
3. Enjoy as Gitlab pipeline is triggered and produces builds for various platforms.

## Tips & Problems 
- if `changelog.md` was forgotten to be updated before tagging - do not modify the existing tags or releases, just commit changes and go on.
- If there are some problems with the release and there's a need to redo it:
    -   Delete the tag
    -   Re-create the release tag
    -   Manually delete artifacts in maven, swift bridge, libdrop-build as build artifacts will be already stored
    -   Proceed as usual

# Tests
## Whole testsuite
```sh
make -C test
```

## Individual tests
Testsuite takes a long time to complete so running specific tests might be preferential and much faster while developing. To run a specific testsuite:
```sh
SCENARIO=scenario_name make -C test
```

## Code Coverage
Before running the coverage you need to install the `rustfilt` demangler and `grcov` tool.
```sh
cargo install rustfilt grcov
```

You also need to include the `llvm-tools-preview` component
```sh
rustup component add llvm-tools-preview
```

### Single test
```sh
SCENARIO=scenario_name make -C test coverage
```

### All testcases
```sh
make -C test coverage
```

## udrop

udrop is an example client-server to test basic functionality of the library.

## Build and run (server)
A container image can be built with the example binary ready for running:
```sh
run.py server run
```

## Run (client)
```sh
export DROP_SERVER=172.17.0.2
cargo run --example udrop -- -l 0.0.0.0 transfer $DROP_SERVER <path>
```

`<path>` is whatever file or folder you want to transfer to the server.

You can verify the transfer by checking the file system in the server container under `/root/<path>`

## Generating file ids from shell
```sh
echo -n "<absolute file path>" | sha256sum  | cut -d " " -f1 | xxd -ps -r | basenc --base64url | tr -d '='
```

## IPv6 format
Whenever IPv6 is used in a text form e.g. it is stored in the DB, included in the event, or passed as a callback argument we use the [RFC 5952](https://tools.ietf.org/html/rfc5952) for unique text representation.

# Contributing
[CONTRIBUTING.md](CONTRIBUTING.md)

# Licensing
[This project is licensed under the terms of the GNU General Public License v3.0 only](LICENSE)
