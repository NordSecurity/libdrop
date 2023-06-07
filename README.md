# libdrop
libdrop is a library for sending/receiving files, primarily over meshnet, but
WAN is also an option.

# Releasing
- We follow the semver.
- After changes are introduced it might need security review.
- After major version increase we need to do release candidates in the form of `v1.2.3-rc1` to ensure that the final major version upgrade contains as litle broken things as possible
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
## Database development
When developing the database there are two modes:
- `online`: query live database for each query
- `offline`: using pregenerated files in .sqlx to validate the queries

SQLX has `offline` always enabled and based on `DATABASE_URL` env var decides which one to use. Env variable has precedence over .sqlx dir.

For any query changes or new queries the database needs to be set-up. Make sure you're running `sqlx-cli` 0.7
which you can build manually by checking out the repository:

```
touch /tmp/db.sqlite
cd drop-storage
cargo sqlx migrate run --database-url sqlite:///tmp/db.sqlite
SQLX_OFFLINE_DIR="./.sqlx" DATABASE_URL=sqlite:///tmp/db.sqlite cargo sqlx prepare
git add .sqlx
```
To speed up the process `.env` file can be created in `drop-storage` with `DATABASE_URL` set up to always use it.
# Contributing
[CONTRIBUTING.md](CONTRIBUTING.md)

# Licensing
[This project is licensed under the terms of the GNU General Public License v3.0 only](LICENSE)
