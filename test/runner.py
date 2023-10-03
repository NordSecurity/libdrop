#!/usr/bin/python3
import argparse
import json
import math
import os
import re
import subprocess
import sys
import time
import typing
from threading import Semaphore, Thread
from typing import Tuple

import docker
from scenarios import scenarios as all_scenarios

TESTCASE_TIMEOUT = 100
SCENARIOS_AT_ONCE = 100

STDERR_ERR_PATTERNS = [
    ["drop-storage", "ERROR"],
]


def prepare_docker() -> docker.DockerClient:
    # Initialize docker client
    client = docker.DockerClient(base_url="unix://var/run/docker.sock")
    return client


def create_network(client: docker.DockerClient, name: str, num: int):
    subnetv6 = f"fd3e:0e6d:45fe:b0c2:{num:x}::/80"
    gatev6 = f"fd3e:0e6d:45fe:b0c2:{num:x}::1"

    subnetv4 = f"192.168.{num}.0/24"
    gatev4 = f"192.168.{num}.1"

    network = client.networks.create(
        name=name,
        driver="bridge",
        enable_ipv6="true",
        ipam=docker.types.IPAMConfig(
            pool_configs=[
                docker.types.IPAMPool(subnet=subnetv6, gateway=gatev6),
                docker.types.IPAMPool(subnet=subnetv4, gateway=gatev4),
            ],
        ),
    )

    return network


class ContainerHolder:
    def __init__(
        self, container: docker.models.containers.Container, scenario: str, timeout: int
    ):
        self._deadline = time.time() + timeout
        self._container = container
        self._scenario = scenario
        self._exit_code: None | str = None

    def recheck(self):
        self._container.reload()
        if self._container.status == "exited":
            self._exit_code = self._container.attrs["State"]["ExitCode"]
        else:
            if self._deadline < time.time():
                print(f"Killing {self.name()} due to timeout...", flush=True)
                if self._container.status == "running":
                    self._container.stop()

    def success(self) -> Tuple[bool, None | str]:
        return (self._exit_code == 0, self._exit_code)

    def done(self) -> bool:
        return self._exit_code != None

    def name(self) -> str:
        return self._container.name

    def logs(self):
        logs = self._container.logs().decode("utf-8")
        # prepend each log line with container name
        logs = "\n".join([f"{self.name()}: {line}" for line in logs.split("\n")])
        return logs


def run():
    print("*** Test suite launched", flush=True)

    all_tags = []
    for scenario in all_scenarios:
        all_tags += scenario.tags()

    all_tags = list(set(all_tags))
    print(f"* Available tags: {all_tags}")

    scenarios = []
    if "SCENARIO" in os.environ and "TAGS" in os.environ:
        print("TAGS and SCENARIO cannot appear at once")
        exit(3)

    if "TAGS" in os.environ:
        tags = [x.strip() for x in os.environ["TAGS"].split(",")]

        print(f"Will execute scenarios with tags: {tags}")
        for s in all_scenarios:
            if all([tag in s.tags() for tag in tags]):
                scenarios.append(s)

    elif "SCENARIO" in os.environ:
        name = os.environ["SCENARIO"]
        pattern = re.compile(name)

        for s in all_scenarios:
            if pattern.fullmatch(s.id()):
                scenarios.append(s)

        if len(scenarios) == 0:
            print(f"Unrecognized scenario: {name}")
            exit(2)
    else:
        scenarios = all_scenarios

    total_time = 0
    start_time = time.time()
    print(f"Will execute {len(scenarios)} scenario(s): {[s.id() for s in scenarios]}")

    client = prepare_docker()

    scenario_results: dict[str, list[ContainerHolder]] = {}

    networks = []
    already_done = []

    # a semaphore is not actually needed as there's no multithreading
    sem = Semaphore(SCENARIOS_AT_ONCE)

    total_containers = 0
    for s in scenarios:
        total_containers += len(s.runners())

    while True:
        if len(already_done) == len(scenarios):
            break

        for i, scenario in enumerate(scenarios):
            if scenario.id() in already_done:
                continue

            if scenario.id() in scenario_results:
                for container in scenario_results[scenario.id()]:
                    container.recheck()

                if all(
                    [container.done() for container in scenario_results[scenario.id()]]
                ):
                    already_done.append(scenario.id())
                    sem.release()
                continue

            if sem.acquire(blocking=False):
                print(
                    f"Executing scenario {i+1}/{len(scenarios)}({scenario.id()}): {scenario.desc()}. Runners: {scenario.runners()}",
                    flush=True,
                )

                netname = f"net-{scenario.id()}"
                networks.append(create_network(client, netname, i + 1))

                scenario_results[scenario.id()] = []

                for runner in scenario.runners():
                    COMMON_VOLUMES = {}
                    parent_dir = os.path.dirname(os.getcwd())
                    COMMON_VOLUMES[parent_dir] = {"bind": "/libdrop", "mode": "rw"}

                    COMMON_WORKING_DIR = "/libdrop/test"
                    COMMON_CAP_ADD = ["NET_ADMIN"]

                    hostname = f"{runner}-{scenario.id()}"
                    print(f"Starting {hostname}...")
                    LIB_PATH = os.environ["LIB_PATH"]
                    # TODO: would be great to notify each container that all of their peers are online and DNS resolving now works instead of sleeping
                    cmd = f"sh -c 'sleep 5 && ./run.py --runner={runner} --scenario={scenario.id()} --lib={LIB_PATH}'"

                    env = [
                        "RUST_BACKTRACE=1",
                        f"DROP_PEER_REN=DROP_PEER_REN-{scenario.id()}",
                        f"DROP_PEER_STIMPY=DROP_PEER_STIMPY-{scenario.id()}",
                        f"DROP_PEER_GEORGE=DROP_PEER_GEORGE-{scenario.id()}",
                        f"DROP_PEER_REN6=DROP_PEER_REN6-{scenario.id()}",
                        f"DROP_PEER_STIMPY6=DROP_PEER_STIMPY6-{scenario.id()}",
                        f"DROP_PEER_GEORGE6=DROP_PEER_GEORGE6-{scenario.id()}",
                    ]

                    print(f"  running {cmd}", flush=True)
                    container = client.containers.run(
                        image="libdroptestimage",
                        name=f"{hostname}",
                        command=cmd,
                        volumes=COMMON_VOLUMES,
                        working_dir=COMMON_WORKING_DIR,
                        cap_add=COMMON_CAP_ADD,
                        environment=env,
                        hostname=f"{hostname}",
                        detach=True,
                        network=netname,
                    )

                    info = ContainerHolder(container, scenario.id(), TESTCASE_TIMEOUT)
                    scenario_results[scenario.id()].append(info)

        curr_time = time.strftime("%H:%M:%S", time.localtime())

        done_containers = 0
        failed_container_count = 0
        for scenario in scenarios:
            if scenario.id() in scenario_results:
                for container in scenario_results[scenario.id()]:
                    if container.done():
                        done_containers += 1
                        success, reason = container.success()
                        if not success:
                            failed_container_count += 1

        print(
            f"*** Test suite progress: {curr_time}: {done_containers}/{total_containers} containers finished, {failed_container_count} failed",
            flush=True,
        )

        time.sleep(1)

    total_time = round(time.time() - start_time, 2)

    print(
        f"*** Test suite finished in {total_time} seconds, using {SCENARIOS_AT_ONCE} batch scenarios",
        flush=True,
    )
    total_scenarios_count = len(scenarios)
    failed_scenarios = []

    for network in networks:
        network.remove()

    for scenario in scenarios:
        for container in scenario_results[scenario.id()]:
            success, reason = container.success()
            if not success:
                failed_scenarios.append(scenario)
                break

    if len(failed_scenarios) == 0:
        print("Success! All tests passed!", flush=True)
        exit(0)
    else:
        for scenario in failed_scenarios:
            print(
                f"*** Scenario {scenario.id()} failed",
                flush=True,
            )
            print(f"*** Scenario {scenario.id()} logs:", flush=True)
            for container in scenario_results[scenario.id()]:
                print(f"*** Container {container.name()} logs:", flush=True)
                print(container.logs(), flush=True)

        print("Failure summary:", flush=True)
        for scenario in scenarios:
            failed_container_names = []
            for container in scenario_results[scenario.id()]:
                success, reason = container.success()
                if not success:
                    failed_container_names.append(container.name())

            if len(failed_container_names) > 0:
                print(
                    f"*** Scenario {scenario.id()}, failed containers: {failed_container_names}",
                    flush=True,
                )
        print(
            f"*** Test suite results: {total_scenarios_count} scenarios, {len(failed_scenarios)} failed. Succeeded ({round((1.0-(len(failed_scenarios)/total_scenarios_count)) * 100, 2)}%), on average one scenario took {math.ceil(total_time/total_scenarios_count)} seconds",
            flush=True,
        )

        exit(1)


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description="Run drop instance")
    parser.add_argument(
        "--testcase-timeout", required=False, help="timeout for each scenario"
    )
    parser.add_argument(
        "--scenarios-at-once", required=False, help="batch size for scenarios"
    )
    args = parser.parse_args()
    if args.testcase_timeout:
        TESTCASE_TIMEOUT = int(args.testcase_timeout)
    if args.scenarios_at_once:
        SCENARIOS_AT_ONCE = int(args.scenarios_at_once)

    print(f"Running with timeout {TESTCASE_TIMEOUT} and batch size {SCENARIOS_AT_ONCE}")
    run()
