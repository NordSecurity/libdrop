#!/usr/bin/python3

import argparse
import asyncio
import os
import sys

from drop_test.logger import logger
from scenarios import scenarios

from drop_test import ffi


async def main():
    print(f"Python version: {sys.version}", flush=True)

    parser = argparse.ArgumentParser(description="Run drop instance")
    parser.add_argument("--runner", required=True, help="peer name for the scenario")
    parser.add_argument("--scenario", required=True, help="scenario name")

    args = parser.parse_args()

    runner = args.runner
    scenario = args.scenario

    os.environ["LLVM_PROFILE_FILE"] = f"./coverage/{scenario}-{runner}.profraw"

    script = None
    for scn in scenarios:
        if scn._id == scenario:
            script = scn
            break

    if script is None:
        raise Exception("unrecognized scenario", scenario)

    drop = ffi.Drop(ffi.KeysCtx(runner))
    logger.info(f"NordDrop version: {drop.version}")

    exit_code = 0
    try:
        await script.run(runner, drop)
        logger.info("Action completed properly")

    except Exception as e:
        import traceback

        logger.critical(
            f"Action didn't complete as expected: {e} at {traceback.print_exc()}"
        )
        exit_code = 1

    del drop

    sys.exit(exit_code)


if __name__ == "__main__":
    asyncio.run(main())
