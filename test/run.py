#!/usr/bin/python3

import argparse
import asyncio
import os
import shutil
import subprocess
import sys
import typing

from drop_test.logger import logger
from scenarios import scenarios

from drop_test import ffi, config


def prepare_files(files: typing.Dict, symlinks: typing.Dict, dbfiles: typing.Dict):
    os.mkdir("/tmp/received")

    os.mkdir("/tmp/no-permissions")
    os.chmod("/tmp/no-permissions", 0)

    for name in files:
        size: int = files[name].size

        fullpath = f"/tmp/{name}"
        os.makedirs(os.path.dirname(fullpath), exist_ok=True)

        # Granularity is in KB. A meg is too coarse, a byte is too slow for
        # `dd`
        with subprocess.Popen(
            ["dd", "bs=1K", f"count={size}", "if=/dev/urandom", f"of={fullpath}"],
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
        ) as proc:
            proc.communicate()
            if proc.returncode != 0:
                raise Exception("Couldn't create a testfile")

    for name in symlinks:
        target: str = symlinks[name]

        target_fullpath = f"/tmp/{target}"
        symlink_fullpath = f"/tmp/{name}"

        os.makedirs(os.path.dirname(symlink_fullpath), exist_ok=True)
        os.symlink(target_fullpath, symlink_fullpath)

    os.mkdir("/tmp/db")
    for name in dbfiles:
        contents: bytes = dbfiles[name]

        path = f"/tmp/db/{name}"

        f = open(path, "wb")
        f.write(contents)
        f.close()


def cleanup_files(files: typing.Dict):
    try:
        shutil.rmtree("/tmp/received")
    except BaseException:
        raise

    for name in files:
        fullpath = f"/tmp/{name}"
        with subprocess.Popen(
            ["rm", f"{fullpath}"], stdout=subprocess.PIPE, stderr=subprocess.PIPE
        ) as proc:
            proc.communicate()
            if proc.returncode != 0:
                raise Exception("Couldn't create a testfile")


async def main():
    parser = argparse.ArgumentParser(description="Run drop instance")
    parser.add_argument("--runner", required=True, help="peer name for the scenario")
    parser.add_argument("--scenario", required=True, help="scenario name")
    parser.add_argument("--lib", required=True, help="path to library")

    args = parser.parse_args()

    runner = args.runner
    scenario = args.scenario
    lib = args.lib

    script = None
    for scn in scenarios:
        if scn._id == scenario:
            script = scn
            break

    if script is None:
        raise Exception("unrecognized scenario", scenario)

    symlinks = {
        "received/symtest-files/testfile-small": "this-file-does-not-exists.ext",
        "received/symtest-dir": "this-dir-does-not-exists",
    }

    prepare_files(config.FILES, symlinks, config.DBFILES)

    drop = ffi.Drop(lib, ffi.KeysCtx(runner))
    logger.info(f"NordDrop version: {drop.version}")

    try:
        await script.run(runner, drop)
        logger.info("Action completed properly")
        cleanup_files(config.FILES)
        sys.exit(0)
    except Exception as e:
        logger.critical(f"Action didn't complete as expected: {e}")
        cleanup_files(config.FILES)
        sys.exit(1)


if __name__ == "__main__":
    asyncio.run(main())
