# libdrop
libdrop is a library for sending/receiving files, primarily over meshnet, but
WAN is also an option.

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
## Database development
When developing the database, it is strongly encouraged to use the "online" mode of `sqlx`, by setting the environment variable `DATABASE_URL` or providing it in `drop-storage/.env` file, the variable should contain the path to a database file for sqlx to validate your queries against.

Before pushing new code that contains changes to the queries, the `sqlx-data.json` file should be updated by calling `cargo sqlx prepare`, else the CI pipelines will most likely fail.

# Contributing
[CONTRIBUTING.md](CONTRIBUTING.md)

# Licensing
[This project is licensed under the terms of the GNU General Public License v3.0 only](LICENSE)