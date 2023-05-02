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

from drop_test import ffi


def prepare_files(files: typing.Dict, symlinks: typing.Dict):
    os.mkdir("/tmp/received")

    for name in files:
        size: int = files[name]

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
    parser.add_argument("--addr", default="0.0.0.0", help="address to listen on")
    parser.add_argument("--lib", required=True, help="path to library")

    args = parser.parse_args()

    runner = args.runner
    scenario = args.scenario
    addr = args.addr
    lib = args.lib

    script = None
    for scn in scenarios:
        if scn._id == scenario:
            script = scn
            break

    if script is None:
        raise Exception("unrecognized scenario", scenario)

    # in kilobytes
    test_files = {
        "thisisaverylongfilenameusingonlylowercaselettersandnumbersanditcontainshugestringofnumbers01234567890123456789012345678901234567890123456789012345678901234567890123456789012345678901234561234567891234567891234567890123456789012345678901234567890123456.txt": 1
        * 1024,
        "testfile-small": 1 * 1024,
        "testfile-big": 10 * 1024,
        "deep/path/file1.ext1": 1 * 1024,
        "deep/path/file2.ext2": 1 * 1024,
        "deep/another-path/file3.ext3": 1 * 1024,
        "deep/another-path/file4.ext4": 1 * 1024,
        "testfile-bulk-01": 10 * 1024,
        "testfile-bulk-02": 10 * 1024,
        "testfile-bulk-03": 10 * 1024,
        "testfile-bulk-04": 10 * 1024,
        "testfile-bulk-05": 10 * 1024,
        "testfile-bulk-06": 10 * 1024,
        "testfile-bulk-07": 10 * 1024,
        "testfile-bulk-08": 10 * 1024,
        "testfile-bulk-09": 10 * 1024,
        "testfile-bulk-10": 10 * 1024,
        "nested/big/testfile-01": 10 * 1024,
        "nested/big/testfile-02": 10 * 1024,
        "testfile.small.with.complicated.extension": 1 * 1024,
        "with-illegal-char-\x0A-": 1 * 1024,
        "duplicate/testfile-small": 1 * 1024,
        "duplicate/testfile.small.with.complicated.extension": 1 * 1024,
        "zero-sized-file": 0,
    }

    symlinks = {
        "received/symtest-files/testfile-small": "this-file-does-not-exists.ext",
        "received/symtest-dir": "this-dir-does-not-exists",
    }

    prepare_files(test_files, symlinks)

    drop = ffi.Drop(lib, ffi.KeysCtx(runner))
    logger.info(f"NordDrop version: {drop.version}")
    drop.start(addr, runner)

    try:
        await script.run(runner, drop)
        logger.info("Action completed properly")
        cleanup_files(test_files)
        sys.exit(0)
    except Exception as e:
        logger.critical(f"Action didn't complete as expected: {e}")
        cleanup_files(test_files)
        sys.exit(1)


if __name__ == "__main__":
    asyncio.run(main())
