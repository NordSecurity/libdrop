#!/usr/bin/python3

from scenarios import scenarios

import subprocess
import sys
import os
import json

STDERR_ERR_PATTERNS = [
    "DB Error",
]


def run():
    print("*** Test suite launched", flush=True)
    print("This will run test suite")

    failed_scenarios = []

    for scenario in scenarios:
        print(f"Executing scenario '{scenario.id()}'", flush=True)
        my_env = os.environ.copy()
        my_env["SCENARIO"] = scenario.id()

        res = subprocess.run(
            ["docker", "compose", "down", "--remove-orphans", "--timeout", "4"],
            env=my_env,
        )
        if res.returncode != 0:
            print("`docker compose down` was not successful")
            exit(1)

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
        status_json = json.loads(status_json)

        failed = []

        decoded_stderr = stderr.decode("unicode_escape")
        stderr_captured_error = stderr_captured_error = [
            pattern for pattern in STDERR_ERR_PATTERNS if pattern in decoded_stderr
        ]

        for item in status_json:

            service: str = item["Service"]

            if service in scenario.runners() and (
                item["ExitCode"] != 0 or stderr_captured_error
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

            print(f"------------")
            print("", flush=True)

            failed_scenarios.append(f"{scenario.id()} for {failed}")
        else:
            print(f"Scenario '{scenario.id()}' ran successfuly", flush=True)

    if len(failed_scenarios) > 0:
        print(f"Failed scenarios:")
        for s in failed_scenarios:
            print(f"- {s}")
        print("look into logs above for specific output")
        sys.exit(1)
    sys.exit(0)


if __name__ == "__main__":
    run()
