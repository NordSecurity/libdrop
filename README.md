 # libdrop
libdrop is a library for sending/receiving files, primarily over meshnet, but
WAN is also an option.

# Releasing
- We follow the semver.
- After changes are introduced it might need security review. This is especially true if major semver component is increased.
- Before releasing `changelog.md` must be updated with the version name being released.
- After the release the `main` branch should be merged into the `dev` branch

# Tests
## Whole testsuite
```
cd test
LIB_PATH=PATH_TO_LIB_BINARY make run
```

## Individual tests
Testsuite takes a long time to complete so running specific tests might be preferential and much faster while developing. To run a specific testsuite you must have a library already built from the previous step.
```
cd test
docker compose down && SCENARIO="SCENARIO_NAME" LIB_PATH=PATH_TO_LIB_BINARY docker compose up ren stimpy george
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
.
.
