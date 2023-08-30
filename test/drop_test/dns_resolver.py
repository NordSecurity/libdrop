from typing import Callable
import os
import socket
import time


class DNSResolver:  # TODO: the name is a lie, it's more of a PeerResolver
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
            f"Initializing DNS resolver. Looking for peers in {peer_env_vars}",
            flush=True,
        )
        for peer_env_var in peer_env_vars:
            if peer_env_var in os.environ:
                peer = os.environ[peer_env_var]
                self._peer_mappings[peer_env_var] = peer
                print(f"Found peer {peer_env_var}={peer}", flush=True)

        if len(self._peer_mappings) == 0:
            print("No peers found in DNSResolver", flush=True)

        print(
            f"Initialized DNS resolver with {len(self._peer_mappings)} peers",
            flush=True,
        )
        print(f"Initialized DNS resolver with {self._peer_mappings} peers", flush=True)

    def resolve(self, peer: str) -> str:
        hostname = self._peer_mappings[peer]

        if hostname in self._cache:
            return self._cache[hostname]

        for _ in range(5):
            try:
                ip = socket.gethostbyname(hostname)
                self._cache[hostname] = ip
                return ip

            except:
                print(f"Unable to resolve hostname({hostname}), retrying in 1s")

                time.sleep(1)

        raise Exception(f"Unable to resolve hostname {hostname}")

    def reverse_lookup(self, ip: str) -> str:
        for hostname, cached_ip in self._cache.items():
            if cached_ip == ip:
                return hostname.split("-")[0]  # TODO

        raise Exception(f"Could not find hostname for ip {ip}")


dns_resolver = DNSResolver()
