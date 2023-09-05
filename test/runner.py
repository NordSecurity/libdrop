#!/usr/bin/python3
from scenarios import scenarios as all_scenarios

import subprocess
import sys
import os
import json
import time
import math
import re
import docker
from threading import Semaphore, Thread

TESTCASE_TIMEOUT = 30

STDERR_ERR_PATTERNS = [
    ["drop-storage", "ERROR"],
]


def prepare_docker() -> docker.DockerClient:
    # Initialize docker client
    client = docker.DockerClient(base_url="unix://var/run/docker.sock")
    network = client.networks.create(
        "libdrop_test_network", driver="bridge", attachable=True
    )

    # Network creation
    ipv4_net = client.networks.create(
        name="interneciux",
        driver="bridge",
        ipam=docker.types.IPAMConfig(
            pool_configs=[docker.types.IPAMPool(subnet="172.30.0.0/16")]
        ),
    )

    ipv6_net = client.networks.create(
        name="interneciux-v6",
        driver="bridge",
        options={"com.docker.network.enable_ipv6": "true"},
        ipam=docker.types.IPAMConfig(
            pool_configs=[
                docker.types.IPAMPool(
                    subnet="fd3e:0e6d:45fe:b0c2::/64", gateway="fd3e:0e6d:45fe:b0c2::1"
                )
            ]
        ),
    )

    return client


class ContainerInfo:
    def __init__(
        self, container: docker.models.containers.Container, scenario: str, timeout: int
    ):
        self._deadline = time.time() + timeout
        self._missed_deadline = False
        self._container = container
        self._scenario = scenario
        self._exit_code = None

    def recheck(self):
        self._container.reload()
        if self._container.status == "exited":
            self._exit_code = self._container.attrs["State"]["ExitCode"]
        self._missed_deadline = self._deadline < time.time()

    def success(self) -> bool:
        return not self._missed_deadline and self._exit_code == 0

    def done(self) -> bool:
        return self._exit_code != None or self._missed_deadline

    def name(self) -> str:
        return self._container.name

    def run(self):
        self._container.run()

    def logs(self):
        logs = self._container.logs().decode("utf-8")
        # prepend each log line with container name
        logs = "\n".join([f"{self.name()}: {line}" for line in logs.split("\n")])
        return logs


def run():
    print("*** Test suite launched", flush=True)

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

    total_time = 0
    start_time = time.time()
    print(f"Will execute {len(scenarios)} scenario(s): {[s.id() for s in scenarios]}")

    client = prepare_docker()

    results: dict[str, list[ContainerInfo]] = {}

    sem = Semaphore(2)
    for i, scenario in enumerate(scenarios):
        print(
            f"Executing scenario {i+1}/{len(scenarios)}({scenario.id()}): {scenario.desc()}",
            flush=True,
        )

        with sem:
            results[scenario.id()] = []
            for runner in scenario.runners():
                COMMON_VOLUMES = {}
                parent_dir = os.path.dirname(os.getcwd())
                COMMON_VOLUMES[parent_dir] = {"bind": "/libdrop", "mode": "rw"}

                COMMON_WORKING_DIR = "/libdrop/test"
                COMMON_CAP_ADD = ["NET_ADMIN"]

                hostname = f"{runner}-{scenario.id()}"
                print(f"Starting {hostname}...")
                LIB_PATH = os.environ["LIB_PATH"]
                cmd = f"sh -c './run.py --runner={runner} --scenario={scenario.id()} --lib={LIB_PATH}'"

                env = [
                    "RUST_BACKTRACE=1",
                    f"DROP_PEER_REN=DROP_PEER_REN-{scenario.id()}",
                    f"DROP_PEER_STIMPY=DROP_PEER_STIMPY-{scenario.id()}",
                    f"DROP_PEER_GEORGE=DROP_PEER_GEORGE-{scenario.id()}",
                ]

                print(f"Running {cmd}", flush=True)
                container = client.containers.run(
                    image="ghcr.io/nordsecurity/libdrop:libdroptestimage",
                    name=f"{hostname}",
                    command=cmd,
                    volumes=COMMON_VOLUMES,
                    working_dir=COMMON_WORKING_DIR,
                    cap_add=COMMON_CAP_ADD,
                    environment=env,
                    hostname=f"{hostname}",
                    detach=True,
                    network="libdrop_test_network",
                )

                info = ContainerInfo(container, scenario.id(), TESTCASE_TIMEOUT)
                results[scenario.id()].append(info)

    total_time = time.time() - start_time
    # total_cnt is total count of containers in all scenarios
    total_cnt = 0

    # iterate results and count total_cnt of containers
    for scn, info in results.items():
        total_cnt += len(info)

    while True:
        done_containers = 0
        for scn, info in results.items():
            for container in info:
                container.recheck()
                container_name = container.name()
                if container.done():
                    done_containers = done_containers + 1

        curr_time = time.strftime("%H:%M:%S", time.localtime())
        print(
            f"*** Test suite progress: {curr_time} {done_containers}/{total_cnt} containers finished",
            flush=True,
        )
        if done_containers == total_cnt:
            print("All containers finished job")
            failed_scenarios = {}

            for scn, info in results.items():
                failed_scenarios[scn] = []

                for container in info:
                    container_name = container.name()
                    if not container.success():
                        failed_scenarios[scn].append(container_name)

            failed_count = 0
            for scn, info in results.items():
                for container in info:
                    if container.done() and not container.success():
                        failed_count = failed_count + 1

            if failed_count > 0:
                print(
                    f"*** Test suite finished unsuccessfully in {total_time}s",
                    flush=True,
                )

                print(f"Failed scenarios: {len(failed_scenarios)}/{len(scenarios)}")

                print("Failed scenarios and their logs")
                for scn, info in results.items():
                    print("------------------------")
                    print(f"Scenario: {scn}:")
                    for container in info:
                        if not container.success():
                            print("")
                            print("")
                            print(f"Container: {container.name()}")
                            print(f"LOGS BELOW:\n\n{container.logs()}")

                    print("------------------------")

                print(f"Failed scenarios summary:")
                for scn, info in results.items():
                    print(f"  {scn}:")
                    for container in info:
                        if not container.success():
                            print(f"    {container.name()}")
                print("------------------------")

                print(
                    f"*** Test suite finished unsuccessfully in {total_time}s",
                    flush=True,
                )
                sys.exit(1)

            print(f"*** Test suite finished in {round(total_time)}s", flush=True)
            sys.exit(0)

        sleep_between_tests_s = 2
        total_time += sleep_between_tests_s
        time.sleep(sleep_between_tests_s)


if __name__ == "__main__":
    run()
