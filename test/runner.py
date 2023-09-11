#!/usr/bin/python3
from scenarios import scenarios as all_scenarios

import subprocess
import sys
import os
import json
import time
import math
import re

STDERR_ERR_PATTERNS = [
    ["drop-storage", "ERROR"],
]


def run():
    print("*** Test suite launched", flush=True)
    print("This will run test suite")

    scenarios = []
    if "SCENARIO" in os.environ:
        name = os.environ["SCENARIO"]
        pattern = re.compile(name)

        for s in all_scenarios:
            if pattern.fullmatch(s.id()):
                scenarios.append(s)

        if len(scenarios) == 0:
            print(f"Unrecognized scenario: {name}")
            exit(1)
    else:
        scenarios = all_scenarios

    failed_scenarios = []

    print(f"Will execute {len(scenarios)} scenario(s): {[s.id() for s in scenarios]}")
    for i, scenario in enumerate(scenarios):
        time_start = time.time()

        print(
            f"Executing scenario {i+1}/{len(scenarios)}({scenario.id()}): {scenario.desc()}",
            flush=True,
        )
        my_env = os.environ.copy()
        my_env["SCENARIO"] = scenario.id()

        # res = subprocess.run(
        #     ["docker", "compose", "down", "--remove-orphans", "--timeout", "4"],
        #     env=my_env,
        # )
        # if res.returncode != 0:
        #     print("`docker compose down` was not successful")
        #     exit(1)

        args = [
            "docker",
            "compose",
            "up",
            "--force-recreate",
        ]
        args.extend(scenario.runners())

        res = subprocess.Popen(
            args,
            env=my_env,
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
        )

        stdout, stderr = res.communicate()

        status_json = subprocess.check_output(
            ["docker", "compose", "ps", "-a", "--format", "json"], env=my_env
        )

        statuses = []
        for line in status_json.splitlines():
            item = json.loads(line)
            service: str = item["Service"]
            code: int = item["ExitCode"]
            statuses.append((service, code))

        decoded_stderr: str = stderr.decode("unicode_escape")

        stderr_captured_errored_lines = []
        for line in decoded_stderr.splitlines():
            for pattern in STDERR_ERR_PATTERNS:
                if all(phrase in line for phrase in pattern):
                    stderr_captured_errored_lines.append(line)
                    break

        failed = []
        for item in statuses:
            if item[0] in scenario.runners() and (
                item[1] != 0 or len(stderr_captured_errored_lines) > 0
            ):
                failed.append(service)

        if failed:
            print(
                f"Scenario '{scenario.id()}' has failed for runners: {failed}. Check the output below:",
                flush=True,
            )

            print("res=", res)
            print(f"---STDOUT---")
            print(stdout.decode("unicode_escape"))

            print(f"---STDERR---")
            print(decoded_stderr)

            if len(stderr_captured_errored_lines) > 0:
                print(f"---SUSPICIOUS LINES---")
                for line in stderr_captured_errored_lines:
                    print(line)

            print(f"------------")
            print("", flush=True)

            failed_scenarios.append(f"{scenario.id()} for {failed}: {scenario.desc()}")
        else:
            time_taken = math.trunc(time.time() - time_start)
            print(
                f"Scenario '{scenario.id()}' ran successfuly in {time_taken}s",
                flush=True,
            )

    if len(failed_scenarios) > 0:
        print(f"Failed scenarios:")
        for s in failed_scenarios:
            print(f"- {s}")
        print("look into logs above for specific output")
        sys.exit(1)
    sys.exit(0)


if __name__ == "__main__":
    run()
