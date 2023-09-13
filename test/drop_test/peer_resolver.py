from typing import Callable
import os
import socket
import time


class Peer:
    def __init__(self, name: str, scenario: str):
        self._name = name
        self._scenario = scenario

    @classmethod
    def from_hostname(cls, hostname: str) -> "Peer":
        print(f"Creating peer from hostname {hostname}", flush=True)
        (name, scenario) = hostname.split("-", maxsplit=1)
        return cls(name, scenario)

    def get_hostname(self) -> str:
        return f"{self._name}-{self._scenario}"

    def get_name(self) -> str:
        return self._name

    def get_scenario(self) -> str:
        return self._scenario


class PEERResolver:
    def __init__(self):
        self._peer_mappings = {}
        self._cache = {}

        # peers do not know initially how their peers are named, and because
        # everyone lives in the same network we need to dynamically allocate
        # hostnames. This mapping maps those peers to their hostnames based on
        # environment variables.
        peer_env_vars = [
            "DROP_PEER_REN",
            "DROP_PEER_STIMPY",
            "DROP_PEER_GEORGE",
            "DROP_PEER_REN6",
            "DROP_PEER_STIMPY6",
            "DROP_PEER_GEORGE6",
        ]

        print(
            f"Initializing DNS resolver. Looking for peer mappings: {peer_env_vars}",
            flush=True,
        )
        for peer_env_var in peer_env_vars:
            if peer_env_var in os.environ:
                peer = os.environ[peer_env_var]
                self._peer_mappings[peer_env_var] = peer
                print(f"Found peer mapping {peer_env_var}={peer}", flush=True)

        if len(self._peer_mappings) == 0:
            print("No peers found in DNSResolver", flush=True)

        print(
            f"Initialized DNS resolver with {len(self._peer_mappings)} peers",
            flush=True,
        )

    # TOOD: Hack assumes IPV6 if peer name contains "6"
    def resolve(self, peer: str) -> str:
        hostname = self._peer_mappings[peer]

        if hostname in self._cache:
            return self._cache[hostname]

        ipv6 = "6" in peer

        for _ in range(5):
            try:
                if ipv6:
                    ip = socket.getaddrinfo(hostname, 49111, socket.AF_INET6)
                else:
                    ip = socket.getaddrinfo(hostname, 49111, socket.AF_INET)

                host = ip[0][4][0]

                self._cache[hostname] = host
                return host

            except:
                print(f"Unable to resolve hostname({hostname}), retrying in 1s")

                time.sleep(1)

        raise Exception(f"Unable to resolve hostname {hostname}")

    def reverse_lookup(self, ip: str) -> str:
        for hostname, cached_ip in self._cache.items():
            if cached_ip == ip:
                return Peer.from_hostname(hostname).get_name()

        raise Exception(f"Could not find hostname for ip {ip}")


peer_resolver = PEERResolver()